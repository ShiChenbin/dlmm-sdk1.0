use crate::pair_config::get_pair_config;
use crate::pair_config::PairConfig;
use crate::state::get_decimals;
use crate::state::AllPosition;
use crate::state::PositionInfo;
use crate::state::SinglePosition;
use crate::utils::parse_swap_event;
use crate::utils::send_tx;
use crate::utils::simulate_transaction;
use crate::utils::{create_program, get_epoch_sec, get_or_create_ata};
use crate::MarketMakingMode;
use anchor_client::anchor_lang::Space;
use anchor_client::solana_client::rpc_filter::{Memcmp, RpcFilterType};
use anchor_client::solana_sdk::compute_budget::ComputeBudgetInstruction;
use anchor_client::solana_sdk::instruction::Instruction;
use anchor_client::solana_sdk::signature::Signer;
use anchor_client::solana_sdk::signature::{read_keypair_file, Keypair};
use anchor_client::{solana_sdk::pubkey::Pubkey, Cluster, Program};
use anchor_lang::prelude::AccountMeta;
use anchor_lang::AccountDeserialize;
use anchor_lang::InstructionData;
use anchor_lang::ToAccountMetas;
use anchor_spl::associated_token::get_associated_token_address;
use anchor_spl::token::spl_token;
use anchor_spl::token::Mint;
use anchor_spl::token::TokenAccount;
use anyhow::{self, Result, Error, Context};
use lb_clmm::accounts;
use lb_clmm::constants::MAX_BIN_PER_ARRAY;
use lb_clmm::constants::MAX_BIN_PER_POSITION;
use lb_clmm::events::Swap as SwapEvent;
use lb_clmm::instruction;
use lb_clmm::instructions::deposit::*;
use lb_clmm::math::safe_math::SafeMath;
use lb_clmm::state::{bin::BinArray, lb_pair::LbPair, position::PositionV2};
use lb_clmm::utils::pda;
use lb_clmm::utils::pda::*;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::Mutex;
pub struct Core {
    pub provider: Cluster,
    pub wallet: Option<String>,
    pub owner: Pubkey,
    pub config: Vec<PairConfig>,
    pub state: Arc<Mutex<AllPosition>>,
}

impl Core {
    /// 刷新config中的所有交易对的状态信息，包括流动性仓位、bin arrays等数据
    pub async fn refresh_state(&self) -> Result<()> {
        println!("===== refresh_state开始 =====");
        
        println!("创建Program对象...");
        // 创建与DeFi协议只能合约交互的program对象，用于后续的RPC调用
        let program: Program<Arc<Keypair>> = create_program(
            self.provider.to_string(),
            self.provider.to_string(),
            lb_clmm::ID,
            Arc::new(Keypair::new()),
        )?;
        println!("Program对象创建成功");

        for (i, pair) in self.config.iter().enumerate() {
            println!("处理交易对 #{}: {}", i+1, pair.pair_address);
            
            // 将pair_address从字符串转换为Pubkey类型
            let pair_address = Pubkey::from_str(&pair.pair_address).unwrap();
            println!("尝试获取LbPair账户...");
            
            // 获取原始账户数据并检查
            println!("先检查原始账户数据...");
            let raw_account = program.rpc().get_account(&pair_address);
            match raw_account {
                Ok(account) => {
                    println!("账户数据大小: {} 字节", account.data.len());
                    println!("账户所有者: {}", account.owner);
                    
                    // 检查数据对齐
                    println!("数据起始地址: {:p}", account.data.as_ptr());
                    println!("数据对齐情况: 起始地址 % 8 = {}", (account.data.as_ptr() as usize) % 8);
                },
                Err(e) => println!("获取原始账户失败: {}", e),
            }
            
            // 现在尝试解析账户
            println!("尝试解析为LbPair...");
            println!("pair_address: {}", pair_address);
            let lb_pair_state_result = program.account::<LbPair>(pair_address).await;
            
            match lb_pair_state_result {
                Ok(lb_pair_state) => {
                    println!("成功获取LbPair");
                    // 打印一些LbPair状态信息
                    println!("token_x_mint: {}", lb_pair_state.token_x_mint);
                    println!("token_y_mint: {}", lb_pair_state.token_y_mint);
                    
                    println!("尝试获取用户的Position信息...");
                    let position_states_result = program
                        .accounts::<PositionV2>(vec![
                            RpcFilterType::DataSize((8 + PositionV2::INIT_SPACE) as u64),
                            RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
                                8 + 32,
                                self.owner.to_bytes().to_vec(),
                            )),
                            RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
                                8,
                                pair_address.to_bytes().to_vec(),
                            )),
                        ]).await;
                    
                    match position_states_result {
                        Ok(mut position_states) => {
                            println!("找到 {} 个Position", position_states.len());
                            
                            let mut position_pks = vec![];
                            let mut positions = vec![];
                            let mut min_bin_id = 0;
                            let mut max_bin_id = 0;
                            let mut bin_arrays = HashMap::new();
                            
                            if position_states.len() > 0 {
                                println!("对Position进行排序...");
                                // sort position by bin id
                                position_states
                                    .sort_by(|a, b| a.1.lower_bin_id.partial_cmp(&b.1.lower_bin_id).unwrap());

                                min_bin_id = position_states[0].1.lower_bin_id;
                                max_bin_id = position_states[position_states.len() - 1].1.upper_bin_id;
                                
                                println!("处理Position的bin信息，min_bin_id={}, max_bin_id={}", min_bin_id, max_bin_id);
                                
                                for position in position_states.iter() {
                                    position_pks.push(position.0);
                                    positions.push(position.1);
                                }
                                
                                println!("处理BinArray...");
                                let mut bin_arrays_indexes = vec![];
                                for (idx, (_pk, position)) in position_states.iter().enumerate() {
                                    println!("处理Position #{}, lower_bin_id={}, upper_bin_id={}", 
                                        idx+1, position.lower_bin_id, position.upper_bin_id);
                                    
                                    for i in position.lower_bin_id..=position.upper_bin_id {
                                        let bin_array_index_result = BinArray::bin_id_to_bin_array_index(i);
                                        
                                        match bin_array_index_result {
                                            Ok(bin_array_index) => {
                                                if bin_arrays_indexes.contains(&bin_array_index) {
                                                    continue;
                                                }
                                                bin_arrays_indexes.push(bin_array_index);
                                                
                                                println!("处理bin_id={}, bin_array_index={}", i, bin_array_index);
                                                let (bin_array_pk, _bump) =
                                                    pda::derive_bin_array_pda(pair_address, bin_array_index.into());
                                                
                                                println!("获取BinArray账户: {}", bin_array_pk);
                                                let bin_array_result = program.account::<BinArray>(bin_array_pk).await;
                                                
                                                match bin_array_result {
                                                    Ok(bin_array_state) => {
                                                        println!("成功获取BinArray");
                                                        bin_arrays.insert(bin_array_pk, bin_array_state);
                                                    },
                                                    Err(e) => {
                                                        println!("❌ 获取BinArray失败: {}", e);
                                                        println!("尝试检查原始账户数据...");
                                                        match program.rpc().get_account(&bin_array_pk) {
                                                            Ok(acct) => println!("原始数据大小: {}, 所有者: {}", 
                                                                    acct.data.len(), acct.owner),
                                                            Err(e2) => println!("获取原始账户也失败: {}", e2),
                                                        }
                                                        return Err(e.into());
                                                    }
                                                }
                                            },
                                            Err(e) => {
                                                println!("❌ bin_id_to_bin_array_index失败: {}", e);
                                                return Err(e.into());
                                            }
                                        }
                                    }
                                }
                            } else {
                                println!("没有Position，跳过BinArray处理");
                            }
                            
                            println!("更新状态...");
                            let mut all_state = self.state.lock().unwrap();
                            let state = all_state.all_positions.get_mut(&pair_address).unwrap();
                            state.lb_pair_state = lb_pair_state;
                            state.bin_arrays = bin_arrays;
                            state.position_pks = position_pks;
                            state.positions = positions;
                            state.min_bin_id = min_bin_id;
                            state.max_bin_id = max_bin_id;
                            state.last_update_timestamp = get_epoch_sec();
                            println!("交易对状态更新完成");
                        },
                        Err(e) => {
                            println!("获取Position失败，可能是账户不存在或余额不足: {}", e);
                            println!("将视为没有Position继续处理");
                            
                            // 更新状态，但设置为空Position
                            let mut all_state = self.state.lock().unwrap();
                            let state = all_state.all_positions.get_mut(&pair_address).unwrap();
                            state.lb_pair_state = lb_pair_state;
                            state.bin_arrays = HashMap::new();
                            state.position_pks = vec![];
                            state.positions = vec![];
                            state.min_bin_id = 0;
                            state.max_bin_id = 0;
                            state.last_update_timestamp = get_epoch_sec();
                            println!("交易对状态更新完成(无Position)");
                            
                            // 返回成功而不是错误
                            return Ok(());
                        }
                    }
                },
                Err(e) => {
                    println!("❌ 获取LbPair失败: {}", e);
                    return Err(e.into());
                }
            }
        }
        
        println!("===== refresh_state完成 =====");
        Ok(())
    }
    /// 获取并缓存所有代币的基本信息(如精度等)
    pub fn fetch_token_info(&self) -> Result<()> {
        println!("===== fetch_token_info开始 =====");
        
        println!("获取所有代币地址...");
        let token_mints = self.get_all_token_mints();
        println!("需要获取 {} 个代币信息", token_mints.len());
        
        println!("创建Program对象...");
        let program: Program<Arc<Keypair>> = create_program(
            self.provider.to_string(),
            self.provider.to_string(),
            lb_clmm::ID,
            Arc::new(Keypair::new()),
        )?;
        
        println!("批量获取代币账户...");
        let accounts_result = program.rpc().get_multiple_accounts(&token_mints);
        
        match accounts_result {
            Ok(accounts) => {
                println!("成功获取 {} 个账户", accounts.len());
                
                let mut tokens = HashMap::new();
                for (i, &token_pk) in token_mints.iter().enumerate() {
                    println!("处理代币 #{}: {}", i+1, token_pk);
                    
                    if let Some(account_option) = accounts.get(i) {
                        if let Some(account) = account_option {
                            println!("账户数据大小: {} 字节", account.data.len());
                            println!("账户所有者: {}", account.owner);
                            
                            // 检查数据对齐
                            println!("数据起始地址: {:p}", account.data.as_ptr());
                            println!("数据对齐情况: 起始地址 % 8 = {}", (account.data.as_ptr() as usize) % 8);
                            
                            println!("尝试反序列化为Mint...");
                            match Mint::try_deserialize(&mut account.data.as_ref()) {
                                Ok(mint) => {
                                    println!("反序列化成功");
                                    tokens.insert(token_pk, mint);
                                },
                                Err(e) => {
                                    println!("❌ 反序列化失败: {}", e);
                                    // 不返回错误，继续处理其他代币
                                }
                            }
                        } else {
                            println!("账户不存在");
                        }
                    }
                }
                
                println!("更新代币状态...");
                let mut state = self.state.lock().unwrap();
                state.tokens = tokens;
                println!("状态更新完成");
            },
            Err(e) => {
                println!("❌ 获取代币账户失败: {}", e);
                return Err(e.into());
            }
        }
        
        println!("===== fetch_token_info完成 =====");
        Ok(())
    }
    /// 获取所有交易对中涉及的代币地址列表
    pub fn get_all_token_mints(&self) -> Vec<Pubkey> {
        let state = self.state.lock().unwrap();
        let mut token_mints = vec![];
        for (_, position) in state.all_positions.iter() {
            token_mints.push(position.lb_pair_state.token_x_mint);
            token_mints.push(position.lb_pair_state.token_y_mint);
        }
        token_mints.sort_unstable();
        token_mints.dedup();
        token_mints
    }

    /// 获取指定交易对的仓位状态信息
    pub fn get_position_state(&self, lp_pair: Pubkey) -> SinglePosition {
        let state = self.state.lock().unwrap();
        let position = state.all_positions.get(&lp_pair).unwrap();
        position.clone()
    }

    /// 初始化用户的代币关联账户(ATA)
    pub async fn init_user_ata(&self) -> Result<()> {
        let payer = read_keypair_file(self.wallet.clone().unwrap())
            .map_err(|_| Error::msg("Requires a keypair file"))?;
        let program: Program<Arc<Keypair>> = create_program(
            self.provider.to_string(),
            self.provider.to_string(),
            spl_token::ID,
            Arc::new(Keypair::new()),
        )?;
        let token_mints = self.get_all_token_mints();
        for &token_mint_pk in token_mints.iter() {
            get_or_create_ata(&program, token_mint_pk, payer.pubkey(), &payer).await?;
        }
        Ok(())
    }

    /// 从指定交易对中撤出所有流动性
    pub async fn withdraw(&self, state: &SinglePosition, is_simulation: bool) -> Result<()> {
        // let state = self.get_state();
        if state.position_pks.len() == 0 {
            return Ok(());
        }
        let (event_authority, _bump) = derive_event_authority_pda();
        let lb_pair = state.lb_pair;
        let payer = read_keypair_file(self.wallet.clone().unwrap())
            .map_err(|_| Error::msg("Requires a keypair file"))?;
        let program: Program<Arc<Keypair>> = create_program(
            self.provider.to_string(),
            self.provider.to_string(),
            lb_clmm::ID,
            Arc::new(Keypair::new()),
        )?;
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

            if is_simulation {
                let response =
                    simulate_transaction(vec![&payer], payer.pubkey(), &program, &builder)?;
                println!("{:?}", response);
            } else {
                let signature = send_tx(vec![&payer], payer.pubkey(), &program, &builder)?;
                info!("close popsition {position} {signature}");
            }
        }

        Ok(())
    }

    /// 在交易对中执行代币兑换操作
    async fn swap(
        &self,
        state: &SinglePosition,
        amount_in: u64,
        swap_for_y: bool,
        is_simulation: bool,
    ) -> Result<SwapEvent> {
        // let state = self.get_state();
        let lb_pair_state = state.lb_pair_state;
        let lb_pair = state.lb_pair;
        let active_bin_array_idx = BinArray::bin_id_to_bin_array_index(lb_pair_state.active_id)?;

        let payer = read_keypair_file(self.wallet.clone().unwrap())
            .map_err(|_| Error::msg("Requires a keypair file"))?;
        let program: Program<Arc<Keypair>> = create_program(
            self.provider.to_string(),
            self.provider.to_string(),
            lb_clmm::ID,
            Arc::new(Keypair::new()),
        )?;
        let (bin_array_0, _bump) = derive_bin_array_pda(lb_pair, active_bin_array_idx as i64);

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

        let (event_authority, _bump) =
            Pubkey::find_program_address(&[b"__event_authority"], &lb_clmm::ID);

        let accounts = accounts::Swap {
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

        let ix = instruction::Swap {
            amount_in,
            min_amount_out: state.get_min_out_amount_with_slippage_rate(amount_in, swap_for_y)?,
        };

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

        let compute_budget_ix = ComputeBudgetInstruction::set_compute_unit_limit(1_400_000);

        let builder = program.request();
        let builder = builder
            .instruction(compute_budget_ix)
            .accounts(accounts)
            .accounts(remaining_accounts)
            .args(ix);

        if is_simulation {
            let response = simulate_transaction(vec![&payer], payer.pubkey(), &program, &builder)?;
            println!("{:?}", response);
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

        let signature = send_tx(vec![&payer], payer.pubkey(), &program, &builder)?;
        info!("swap {amount_in} {swap_for_y} {signature}");

        // TODO should handle if cannot get swap eevent
        let swap_event = parse_swap_event(&program, signature)?;

        Ok(swap_event)
    }

    /// 向交易对添加流动性
    pub async fn deposit(
        &self,
        state: &SinglePosition,
        amount_x: u64,
        amount_y: u64,
        active_id: i32,
        is_simulation: bool,
    ) -> Result<()> {
        // let state = self.get_state();
        let payer = read_keypair_file(self.wallet.clone().unwrap())
            .map_err(|_| Error::msg("Requires a keypair file"))?;
        let program: Program<Arc<Keypair>> = create_program(
            self.provider.to_string(),
            self.provider.to_string(),
            lb_clmm::ID,
            Arc::new(Keypair::new()),
        )?;
        let lower_bin_id = active_id - (MAX_BIN_PER_ARRAY as i32).checked_div(2).unwrap();
        let upper_bin_id = lower_bin_id
            .checked_add(MAX_BIN_PER_ARRAY as i32)
            .unwrap()
            .checked_sub(1)
            .unwrap();
        let lower_bin_array_idx = BinArray::bin_id_to_bin_array_index(lower_bin_id)?;
        let upper_bin_array_idx = lower_bin_array_idx.checked_add(1).unwrap();

        let lb_pair = state.lb_pair;

        let mut instructions = vec![ComputeBudgetInstruction::set_compute_unit_limit(1_400_000)];
        for idx in lower_bin_array_idx..=upper_bin_array_idx {
            // Initialize bin array if not exists
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

        let position_kp = Keypair::new();
        let position = position_kp.pubkey();
        let (event_authority, _bump) =
            Pubkey::find_program_address(&[b"__event_authority"], &lb_clmm::ID);

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

        // TODO implement add liquidity by strategy imbalance
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
        let builder = program.request();
        let builder = instructions
            .into_iter()
            .fold(builder, |bld, ix| bld.instruction(ix));

        if is_simulation {
            let simulate_tx = simulate_transaction(
                vec![&payer, &position_kp],
                payer.pubkey(),
                &program,
                &builder,
            )
            .map_err(|_| Error::msg("Cannot simulate tx"))?;
            info!("deposit {amount_x} {amount_y} {position} {:?}", simulate_tx);
        } else {
            let signature = send_tx(
                vec![&payer, &position_kp],
                payer.pubkey(),
                &program,
                &builder,
            )?;
            info!("deposit {amount_x} {amount_y} {position} {signature}");
        }

        Ok(())
    }

    /// 计算实际可存入的代币数量(考虑用户余额限制)
    pub async fn get_deposit_amount(
        &self,
        position: &SinglePosition,
        amount_x: u64,
        amount_y: u64,
    ) -> Result<(u64, u64)> {
        // let state = self.get_state();
        let lb_pair_state = position.lb_pair_state;

        let payer = read_keypair_file(self.wallet.clone().unwrap())
            .map_err(|_| Error::msg("Requires a keypair file"))?;

        let program: Program<Arc<Keypair>> = create_program(
            self.provider.to_string(),
            self.provider.to_string(),
            lb_clmm::ID,
            Arc::new(Keypair::new()),
        )?;
        let user_token_x =
            get_associated_token_address(&payer.pubkey(), &lb_pair_state.token_x_mint);
        let user_token_y =
            get_associated_token_address(&payer.pubkey(), &lb_pair_state.token_y_mint);

        let user_token_x_state: TokenAccount = program.account(user_token_x).await?;
        let user_token_y_state: TokenAccount = program.account(user_token_y).await?;

        // compare with current balance
        let amount_x = if amount_x > user_token_x_state.amount {
            user_token_x_state.amount
        } else {
            amount_x
        };

        let amount_y = if amount_y > user_token_y_state.amount {
            user_token_y_state.amount
        } else {
            amount_y
        };

        Ok((amount_x, amount_y))
    }

    /// 获取所有交易对的仓位信息
    pub fn get_all_positions(&self) -> Vec<SinglePosition> {
        let state = self.state.lock().unwrap();
        let mut positions = vec![];
        for (_, position) in &state.all_positions {
            positions.push(position.clone());
        }
        positions
    }

    /// 获取所有代币的信息
    pub fn get_all_tokens(&self) -> HashMap<Pubkey, Mint> {
        let state = self.state.lock().unwrap();
        state.tokens.clone()
    }
    /// 检查并执行价格范围调整(自动再平衡)
    pub async fn check_shift_price_range(&self) -> Result<()> {
        let all_positions = self.get_all_positions();
        for position in all_positions.iter() {
            let pair_config = get_pair_config(&self.config, position.lb_pair);
            // check whether out of price range
            // let state = self.get_state();
            if pair_config.mode == MarketMakingMode::ModeRight
                && position.lb_pair_state.active_id > position.max_bin_id
            {
                self.shift_right(&position).await?;
                self.inc_rebalance_time(position.lb_pair);
            }

            if pair_config.mode == MarketMakingMode::ModeLeft
                && position.lb_pair_state.active_id < position.min_bin_id
            {
                self.shift_left(&position).await?;
                self.inc_rebalance_time(position.lb_pair);
            }
            if pair_config.mode == MarketMakingMode::ModeBoth {
                if position.lb_pair_state.active_id < position.min_bin_id {
                    self.shift_left(&position).await?;
                    self.inc_rebalance_time(position.lb_pair);
                } else if position.lb_pair_state.active_id > position.max_bin_id {
                    self.shift_right(&position).await?;
                    self.inc_rebalance_time(position.lb_pair);
                }
            }
        }

        Ok(())
    }

    /// 向右移动价格范围(当价格上涨超出范围时)
    async fn shift_right(&self, state: &SinglePosition) -> Result<()> {
        let pair_config = get_pair_config(&self.config, state.lb_pair);
        // validate that y amount is zero
        info!("shift right {}", state.lb_pair);
        let position = state.get_positions()?;
        if position.amount_x != 0 {
            return Err(Error::msg("Amount x is not zero"));
        }

        info!("withdraw {}", state.lb_pair);
        // withdraw
        self.withdraw(state, false).await?;

        // buy base
        let amount_y_for_buy = position
            .amount_y
            .safe_div(2)
            .map_err(|_| Error::msg("Math is overflow"))?;
        let (amount_x, amount_y) = if amount_y_for_buy != 0 {
            info!("swap {}", state.lb_pair);
            let swap_event = self.swap(state, amount_y_for_buy, false, false).await?;
            (swap_event.amount_out, position.amount_y - amount_y_for_buy)
        } else {
            (pair_config.x_amount, pair_config.y_amount)
        };

        // deposit again, just test with 1 position only
        info!("deposit {}", state.lb_pair);
        match self
            .deposit(
                state,
                amount_x,
                amount_y,
                state.lb_pair_state.active_id,
                false,
            )
            .await
        {
            Err(_) => {
                self.deposit(
                    state,
                    amount_x,
                    amount_y,
                    state.lb_pair_state.active_id,
                    true,
                )
                .await?;
            }
            _ => {}
        }
        info!("refresh state {}", state.lb_pair);
        // fetch positions again
        self.refresh_state().await?;
        Ok(())
    }
    /// 向左移动价格范围(当价格下跌超出范围时)
    async fn shift_left(&self, state: &SinglePosition) -> Result<()> {
        let pair_config = get_pair_config(&self.config, state.lb_pair);
        info!("shift left {}", state.lb_pair);
        // validate that y amount is zero
        let position = state.get_positions()?;
        if position.amount_y != 0 {
            return Err(Error::msg("Amount y is not zero"));
        }
        info!("withdraw {}", state.lb_pair);
        // withdraw
        self.withdraw(state, false).await?;

        // sell base
        let amount_x_for_sell = position
            .amount_x
            .safe_div(2)
            .map_err(|_| Error::msg("Math is overflow"))?;
        let (amount_x, amount_y) = if amount_x_for_sell != 0 {
            info!("swap {}", state.lb_pair);
            let swap_event = self.swap(state, amount_x_for_sell, true, false).await?;
            (position.amount_x - amount_x_for_sell, swap_event.amount_out)
        } else {
            (pair_config.x_amount, pair_config.y_amount)
        };

        // sanity check with real balances
        let (amount_x, amount_y) = self.get_deposit_amount(state, amount_x, amount_y).await?;
        info!("deposit {}", state.lb_pair);
        match self
            .deposit(
                state,
                amount_x,
                amount_y,
                state.lb_pair_state.active_id,
                false,
            )
            .await
        {
            Err(_) => {
                self.deposit(
                    state,
                    amount_x,
                    amount_y,
                    state.lb_pair_state.active_id,
                    true,
                )
                .await?;
            }
            _ => {}
        }

        info!("refresh state {}", state.lb_pair);
        // fetch positions again
        self.refresh_state().await?;
        Ok(())
    }

    /// 增加再平衡计数器
    pub fn inc_rebalance_time(&self, lb_pair: Pubkey) {
        let mut state = self.state.lock().unwrap();
        let state = state.all_positions.get_mut(&lb_pair).unwrap();
        state.inc_rebalance_time();
    }

    /// 获取所有仓位的详细信息(包含代币精度转换后的数据)
    pub fn get_positions(&self) -> Result<Vec<PositionInfo>> {
        let all_positions = self.get_all_positions();
        let tokens = self.get_all_tokens();

        let mut position_infos = vec![];
        for position in all_positions.iter() {
            let x_decimals = get_decimals(position.lb_pair_state.token_x_mint, &tokens);
            let y_decimals = get_decimals(position.lb_pair_state.token_y_mint, &tokens);
            let position_raw = position.get_positions()?;
            position_infos.push(position_raw.to_position_info(x_decimals, y_decimals)?);
        }
        return Ok(position_infos);
    }
}

#[cfg(test)]
mod core_test {
    use super::*;
    use std::env;
    #[tokio::test(flavor = "multi_thread")]
    async fn test_withdraw() {
        let wallet = env::var("MM_WALLET").unwrap();
        let cluster = env::var("MM_CLUSTER").unwrap();
        let payer = read_keypair_file(wallet.clone()).unwrap();

        let lp_pair = Pubkey::from_str("FoSDw2L5DmTuQTFe55gWPDXf88euaxAEKFre74CnvQbX").unwrap();

        let config = vec![PairConfig {
            pair_address: lp_pair.to_string(),
            x_amount: 17000000,
            y_amount: 2000000,
            mode: MarketMakingMode::ModeBoth,
        }];

        let core = &Core {
            provider: Cluster::from_str(&cluster).unwrap(),
            wallet: Some(wallet),
            owner: payer.pubkey(),
            config: config.clone(),
            state: Arc::new(Mutex::new(AllPosition::new(&config))),
        };

        core.refresh_state().await.unwrap();

        let state = core.get_position_state(lp_pair);

        // withdraw
        core.withdraw(&state, true).await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_swap() {
        let wallet = env::var("MM_WALLET").unwrap();
        let cluster = env::var("MM_CLUSTER").unwrap();
        let payer = read_keypair_file(wallet.clone()).unwrap();

        let lp_pair = Pubkey::from_str("FoSDw2L5DmTuQTFe55gWPDXf88euaxAEKFre74CnvQbX").unwrap();

        let config = vec![PairConfig {
            pair_address: lp_pair.to_string(),
            x_amount: 17000000,
            y_amount: 2000000,
            mode: MarketMakingMode::ModeBoth,
        }];

        let core = &Core {
            provider: Cluster::from_str(&cluster).unwrap(),
            wallet: Some(wallet),
            owner: payer.pubkey(),
            config: config.clone(),
            state: Arc::new(Mutex::new(AllPosition::new(&config))),
        };

        core.refresh_state().await.unwrap();

        let state = core.get_position_state(lp_pair);

        core.swap(&state, 1000000, true, true).await.unwrap();
    }
}
