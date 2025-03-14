use crate::state::SinglePosition;
use crate::wallet::WalletManager;
use crate::utils::{parse_swap_event, send_tx, simulate_transaction, adapt_request_builder};
use anchor_client::solana_sdk::compute_budget::ComputeBudgetInstruction;
use anchor_client::solana_sdk::pubkey::Pubkey;
use anchor_client::solana_sdk::signature::Keypair;
use anchor_client::solana_sdk::signer::Signer;
use anchor_client::solana_sdk::instruction::AccountMeta;
use anchor_spl::associated_token::get_associated_token_address;
use anyhow::*;
use lb_clmm::events::Swap as SwapEvent;
use lb_clmm::state::bin::BinArray;
use lb_clmm::utils::pda::*;
use std::sync::Arc;
use log::info;

pub struct Jupiter {
    pub wallet_manager: Arc<WalletManager>,
}

impl Jupiter {
    pub fn new(wallet_manager: Arc<WalletManager>) -> Self {
        Self { wallet_manager }
    }
    
    // 代币交换函数
    pub async fn swap(
        &self,
        state: &SinglePosition,
        amount_in: u64,
        swap_for_y: bool,
        is_simulation: bool,
    ) -> Result<SwapEvent> {
        let lb_pair_state = state.lb_pair_state;
        let lb_pair = state.lb_pair;
        let active_bin_array_idx = BinArray::bin_id_to_bin_array_index(lb_pair_state.active_id)?;

        // 获取支付者密钥对
        let payer = self.wallet_manager.get_keypair()?;
        
        // 创建程序客户端
        let program = self.wallet_manager.create_program(lb_clmm::ID)?;
        
        // 获取活跃bin数组
        let (bin_array_0, _bump) = derive_bin_array_pda(lb_pair, active_bin_array_idx as i64);

        // 根据交易方向确定相关账户地址
        let (user_token_in, user_token_out, bin_array_1, bin_array_2) = if swap_for_y {
            (
                get_associated_token_address(&payer.pubkey(), &lb_pair_state.token_x_mint),
                get_associated_token_address(&payer.pubkey(), &lb_pair_state.token_y_mint),
                derive_bin_array_pda(lb_pair, (active_bin_array_idx - 1) as i64).0,
                derive_bin_array_pda(lb_pair, (active_bin_array_idx - 2) as i64).0,
            )
        } else {
            (
                get_associated_token_address(&payer.pubkey(), &lb_pair_state.token_y_mint),
                get_associated_token_address(&payer.pubkey(), &lb_pair_state.token_x_mint),
                derive_bin_array_pda(lb_pair, (active_bin_array_idx + 1) as i64).0,
                derive_bin_array_pda(lb_pair, (active_bin_array_idx + 2) as i64).0,
            )
        };

        // 检查位图扩展账户
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

        // 派生事件权限
        let (event_authority, _bump) =
            Pubkey::find_program_address(&[b"__event_authority"], &lb_clmm::ID);

        // 准备交易账户
        let accounts = lb_clmm::accounts::Swap {
            lb_pair,
            bin_array_bitmap_extension,
            reserve_x: lb_pair_state.reserve_x,
            reserve_y: lb_pair_state.reserve_y,
            token_x_mint: lb_pair_state.token_x_mint,
            token_y_mint: lb_pair_state.token_y_mint,
            token_x_program: anchor_spl::token::ID,
            token_y_program: anchor_spl::token::ID,
            user: payer.pubkey(),
            user_token_in,
            user_token_out,
            oracle: lb_pair_state.oracle,
            host_fee_in: Some(lb_clmm::ID),
            event_authority,
            program: lb_clmm::ID,
        };

        // 交换指令参数
        let ix = lb_clmm::instruction::Swap {
            amount_in,
            min_amount_out: state.get_min_out_amount_with_slippage_rate(amount_in, swap_for_y)?,
        };

        // 额外的bin数组账户
        let remaining_accounts = vec![
            AccountMeta {
                is_signer: false,
                is_writable: true,
                pubkey: bin_array_0,
            },
            AccountMeta {
                is_signer: false,
                is_writable: true,
                pubkey: bin_array_1,
            },
            AccountMeta {
                is_signer: false,
                is_writable: true,
                pubkey: bin_array_2,
            },
        ];

        // 计算预算指令
        let compute_budget_ix = ComputeBudgetInstruction::set_compute_unit_limit(1_400_000);

        // 构建请求
        let builder = program.request();
        let builder = builder
            .instruction(compute_budget_ix)
            .accounts(accounts)
            .accounts(remaining_accounts)
            .args(ix);
        
        // 使用适配器包装builder
        let adapter = adapt_request_builder(builder);

        // 模拟或执行交易
        if is_simulation {
            let response = simulate_transaction(vec![&payer], payer.pubkey(), &program, &adapter)?;
            info!("模拟交换: {:?}", response);
            return Ok(SwapEvent {
                lb_pair: Pubkey::default(),
                from: Pubkey::default(),
                start_bin_id: 0,
                end_bin_id: 0,
                amount_in: 0,
                amount_out: 0,
                swap_for_y,
                fee: 0,
                protocol_fee: 0,
                fee_bps: 0,
                host_fee: 0,
            });
        }

        // 发送交易
        let signature = send_tx(vec![&payer], payer.pubkey(), &program, &adapter)?;
        info!("交换成功: {amount_in} {swap_for_y} {signature}");

        // 解析交换事件
        let swap_event = parse_swap_event(&program, signature)?;

        Ok(swap_event)
    }
}