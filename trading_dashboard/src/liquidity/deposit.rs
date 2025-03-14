use crate::state::SinglePosition;
use crate::wallet::WalletManager;
use crate::utils::{send_tx, simulate_transaction, adapt_request_builder};
use anchor_client::solana_sdk::compute_budget::ComputeBudgetInstruction;
use anchor_client::solana_sdk::instruction::Instruction;
use anchor_client::solana_sdk::signature::Keypair;
use anchor_client::solana_sdk::pubkey::Pubkey;
use anchor_lang::InstructionData;
use anchor_lang::ToAccountMetas;
use anchor_spl::associated_token::get_associated_token_address;
use anyhow::{Result, anyhow};
use lb_clmm::accounts;
use lb_clmm::constants::{MAX_BIN_PER_ARRAY, MAX_BIN_PER_POSITION};
use lb_clmm::instruction;
use lb_clmm::instructions::deposit::*;
use lb_clmm::state::bin::BinArray;
use lb_clmm::state::*;
use lb_clmm::utils::pda::*;
use lb_clmm::state::bin_array_bitmap_extension::*;
use std::sync::Arc;
use log::info;
use anchor_client::solana_sdk::signer::Signer;

pub struct Deposit {
    pub wallet_manager: Arc<WalletManager>,
}

impl Deposit {
    pub fn new(wallet_manager: Arc<WalletManager>) -> Self {
        Self { wallet_manager }
    }
    
    // 向流动性池添加流动性的函数
    pub async fn deposit(
        &self,
        state: &SinglePosition,       // 单个仓位的状态
        amount_x: u64,                // 要存入的X代币数量
        amount_y: u64,                // 要存入的Y代币数量
        active_id: i32,               // 活跃的bin ID
        is_simulation: bool,          // 是否为模拟操作(不实际提交到链上)
    ) -> Result<()> {
        // 获取支付者的密钥对
        let payer = self.wallet_manager.get_keypair()?;
        
        // 创建Solana程序客户端
        let program = self.wallet_manager.create_program(lb_clmm::ID)?;
        
        // 计算仓位的下边界bin ID，通常是活跃ID减去半个bin数组大小
        let lower_bin_id = active_id - (MAX_BIN_PER_ARRAY as i32).checked_div(2).unwrap();
        
        // 计算仓位的上边界bin ID
        let upper_bin_id = lower_bin_id
            .checked_add(MAX_BIN_PER_ARRAY as i32)
            .unwrap()
            .checked_sub(1)
            .unwrap();
        
        // 计算bin数组索引
        let lower_bin_array_idx = BinArray::bin_id_to_bin_array_index(lower_bin_id)?;
        let upper_bin_array_idx = lower_bin_array_idx.checked_add(1).unwrap();

        // 获取流动性池地址
        let lb_pair = state.lb_pair;

        // 创建指令列表
        let mut instructions = vec![ComputeBudgetInstruction::set_compute_unit_limit(1_400_000)];
        
        // 确保bin数组已初始化
        for idx in lower_bin_array_idx..=upper_bin_array_idx {
            let (bin_array, _bump) = derive_bin_array_pda(lb_pair, idx.into());

            if program.rpc().get_account_data(&bin_array).is_err() {
                instructions.push(Instruction {
                    program_id: lb_clmm::ID,
                    accounts: accounts::InitializeBinArray {
                        bin_array,
                        funder: payer.pubkey(),
                        lb_pair,
                        system_program: anchor_client::solana_sdk::system_program::ID,
                    }
                    .to_account_metas(None),
                    data: instruction::InitializeBinArray { index: idx.into() }.data(),
                })
            }
        }

        // 创建新的仓位密钥对
        let position_kp = Keypair::new();
        let position = position_kp.pubkey();
        
        // 派生事件授权PDA
        let (event_authority, _bump) =
            Pubkey::find_program_address(&[b"__event_authority"], &lb_clmm::ID);

        // 添加初始化仓位指令
        instructions.push(Instruction {
            program_id: lb_clmm::ID,
            accounts: accounts::InitializePosition {
                lb_pair,
                payer: payer.pubkey(),
                position,
                owner: payer.pubkey(),
                rent: anchor_client::solana_sdk::sysvar::rent::ID,
                system_program: anchor_client::solana_sdk::system_program::ID,
                event_authority,
                program: lb_clmm::ID,
            }
            .to_account_metas(None),
            data: instruction::InitializePosition {
                lower_bin_id,
                width: MAX_BIN_PER_POSITION as i32,
            }
            .data(),
        });
        
        // 准备添加流动性的指令
        let (bin_array_bitmap_extension, _bump) = derive_bin_array_bitmap_extension(lb_pair);
        let bin_array_bitmap_extension = if program
            .rpc()
            .get_account(&bin_array_bitmap_extension)
            .is_err()
        {
            None
        } else {
            Some(bin_array_bitmap_extension)
        };
        
        let (bin_array_lower, _bump) = derive_bin_array_pda(lb_pair, lower_bin_array_idx.into());
        let (bin_array_upper, _bump) = derive_bin_array_pda(lb_pair, upper_bin_array_idx.into());
        let lb_pair_state = state.lb_pair_state;
        let user_token_x =
            get_associated_token_address(&payer.pubkey(), &lb_pair_state.token_x_mint);
        let user_token_y =
            get_associated_token_address(&payer.pubkey(), &lb_pair_state.token_y_mint);

        // 添加流动性指令
        instructions.push(Instruction {
            program_id: lb_clmm::ID,
            accounts: accounts::ModifyLiquidity {
                lb_pair,
                position,
                bin_array_bitmap_extension,
                bin_array_lower,
                bin_array_upper,
                sender: payer.pubkey(),
                event_authority,
                program: lb_clmm::ID,
                reserve_x: lb_pair_state.reserve_x,
                reserve_y: lb_pair_state.reserve_y,
                token_x_mint: lb_pair_state.token_x_mint,
                token_y_mint: lb_pair_state.token_y_mint,
                user_token_x,
                user_token_y,
                token_x_program: anchor_spl::token::ID,
                token_y_program: anchor_spl::token::ID,
            }
            .to_account_metas(None),
            data: instruction::AddLiquidityByStrategy {
                liquidity_parameter: LiquidityParameterByStrategy {
                    amount_x,
                    amount_y,
                    active_id: lb_pair_state.active_id,
                    max_active_bin_slippage: 3,
                    strategy_parameters: StrategyParameters {
                        min_bin_id: lower_bin_id,
                        max_bin_id: upper_bin_id,
                        strategy_type: StrategyType::SpotBalanced,
                        parameteres: [0u8; 64],
                    },
                },
            }
            .data(),
        });
        
        // 构建请求
        let builder = program.request();
        let builder = instructions
            .into_iter()
            .fold(builder, |bld, ix| bld.instruction(ix));
        
        // 使用适配器包装builder
        let adapter = adapt_request_builder(builder);

        // 执行交易或模拟
        if is_simulation {
            let simulate_tx = simulate_transaction(
                vec![&payer, &position_kp],
                payer.pubkey(),
                &program,
                &adapter,
            )
            .map_err(|_| anyhow!("交易模拟失败"))?;
            info!("模拟存款: {amount_x} {amount_y} {position} {:?}", simulate_tx);
        } else {
            let signature = send_tx(
                vec![&payer, &position_kp],
                payer.pubkey(),
                &program,
                &adapter,
            )?;
            info!("存款成功: {amount_x} {amount_y} {position} {signature}");
        }

        Ok(())
    }
}