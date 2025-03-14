use crate::state::SinglePosition;
use crate::wallet::WalletManager;
use crate::utils::{send_tx, simulate_transaction, adapt_request_builder};
use anchor_client::solana_sdk::compute_budget::ComputeBudgetInstruction;
use anchor_client::solana_sdk::pubkey::Pubkey;
use anchor_client::solana_sdk::signature::Keypair;
use anchor_client::solana_sdk::signer::Signer;
use anchor_client::solana_sdk::instruction::Instruction;
use anchor_client::solana_sdk::signer::keypair::read_keypair_file;
use anchor_client::Program;
use anchor_lang::ToAccountMetas;
use anchor_lang::InstructionData;
use anyhow::{Result, anyhow, Context};
use std::sync::Arc;
use log::info;
use lb_clmm::state::bin::BinArray;
use lb_clmm::utils::pda::*;
use lb_clmm::{accounts, instruction};
use anchor_spl::associated_token::get_associated_token_address;

pub struct Withdraw {
    pub wallet_manager: Arc<WalletManager>,
}

impl Withdraw {
    pub fn new(wallet_manager: Arc<WalletManager>) -> Self {
        Self { wallet_manager }
    }
    
    // 完整的提取流动性实现
    pub async fn withdraw(&self, state: &SinglePosition, is_simulation: bool) -> Result<()> {
        // 检查是否有头寸
        if state.position_pks.is_empty() {
            return Ok(());
        }
        
        let (event_authority, _bump) = derive_event_authority_pda();
        let lb_pair = state.lb_pair;
        
        // 获取支付者的密钥对
        let payer = self.wallet_manager.get_keypair()?;
        
        // 创建Solana程序客户端
        let program = self.wallet_manager.create_program(lb_clmm::ID)?;
        
        info!("准备提取流动性...");
        
        let lb_pair_state = state.lb_pair_state;
        for (i, &position) in state.position_pks.iter().enumerate() {
            let position_state = state.positions[i];
            let lower_bin_array_idx =
                BinArray::bin_id_to_bin_array_index(position_state.lower_bin_id)?;
            let upper_bin_array_idx = lower_bin_array_idx.checked_add(1).context("MathOverflow")?;

            let (bin_array_lower, _bump) =
                derive_bin_array_pda(lb_pair, lower_bin_array_idx.into());
            let (bin_array_upper, _bump) =
                derive_bin_array_pda(lb_pair, upper_bin_array_idx.into());

            let user_token_x =
                get_associated_token_address(&payer.pubkey(), &lb_pair_state.token_x_mint);
            let user_token_y =
                get_associated_token_address(&payer.pubkey(), &lb_pair_state.token_y_mint);

            let instructions = vec![
                ComputeBudgetInstruction::set_compute_unit_limit(1_400_000),
                Instruction {
                    program_id: lb_clmm::ID,
                    accounts: accounts::ModifyLiquidity {
                        bin_array_lower,
                        bin_array_upper,
                        lb_pair,
                        bin_array_bitmap_extension: None,
                        position,
                        reserve_x: lb_pair_state.reserve_x,
                        reserve_y: lb_pair_state.reserve_y,
                        token_x_mint: lb_pair_state.token_x_mint,
                        token_y_mint: lb_pair_state.token_y_mint,
                        sender: payer.pubkey(),
                        user_token_x,
                        user_token_y,
                        token_x_program: anchor_spl::token::ID,
                        token_y_program: anchor_spl::token::ID,
                        event_authority,
                        program: lb_clmm::ID,
                    }
                    .to_account_metas(None),
                    data: instruction::RemoveAllLiquidity {}.data(),
                },
                Instruction {
                    program_id: lb_clmm::ID,
                    accounts: accounts::ClaimFee {
                        bin_array_lower,
                        bin_array_upper,
                        lb_pair,
                        sender: payer.pubkey(),
                        position,
                        reserve_x: lb_pair_state.reserve_x,
                        reserve_y: lb_pair_state.reserve_y,
                        token_program: anchor_spl::token::ID,
                        token_x_mint: lb_pair_state.token_x_mint,
                        token_y_mint: lb_pair_state.token_y_mint,
                        user_token_x,
                        user_token_y,
                        event_authority,
                        program: lb_clmm::ID,
                    }
                    .to_account_metas(None),
                    data: instruction::ClaimFee {}.data(),
                },
                Instruction {
                    program_id: lb_clmm::ID,
                    accounts: accounts::ClosePosition {
                        lb_pair,
                        position,
                        bin_array_lower,
                        bin_array_upper,
                        rent_receiver: payer.pubkey(),
                        sender: payer.pubkey(),
                        event_authority,
                        program: lb_clmm::ID,
                    }
                    .to_account_metas(None),
                    data: instruction::ClosePosition {}.data(),
                },
            ];

            let builder = program.request();
            let builder = instructions
                .into_iter()
                .fold(builder, |bld, ix| bld.instruction(ix));
            
            // 使用适配器包装builder
            let adapter = adapt_request_builder(builder);

            if is_simulation {
                let response =
                    simulate_transaction(vec![&payer], payer.pubkey(), &program, &adapter)?;
                println!("{:?}", response);
            } else {
                let signature = send_tx(vec![&payer], payer.pubkey(), &program, &adapter)?;
                info!("close position {position} {signature}");
            }
        }

        Ok(())
    }
}