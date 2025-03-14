// 向流动性池添加流动性的函数
pub async fn deposit(
    &self,
    state: &SinglePosition,       // 单个仓位的状态
    amount_x: u64,                // 要存入的X代币数量
    amount_y: u64,                // 要存入的Y代币数量
    active_id: i32,               // 活跃的bin ID
    is_simulation: bool,          // 是否为模拟操作(不实际提交到链上)
) -> Result<()> {
    // 读取支付者的密钥对文件
    let payer = read_keypair_file(self.wallet.clone().unwrap())
        .map_err(|_| Error::msg("Requires a keypair file"))?;
    
    // 创建Solana程序客户端
    let program: Program<Arc<Keypair>> = create_program(
        self.provider.to_string(),  // RPC提供者URL
        self.provider.to_string(),  // WebSocket提供者URL
        lb_clmm::ID,                // DLMM程序ID
        Arc::new(Keypair::new()),   // 空的签名者(使用payer作为实际签名者)
    )?;
    
    // 计算仓位的下边界bin ID，通常是活跃ID减去半个bin数组大小
    let lower_bin_id = active_id - (MAX_BIN_PER_ARRAY as i32).checked_div(2).unwrap();
    
    // 计算仓位的上边界bin ID，通常是下边界加上bin数组大小再减1
    let upper_bin_id = lower_bin_id
        .checked_add(MAX_BIN_PER_ARRAY as i32)
        .unwrap()
        .checked_sub(1)
        .unwrap();
    
    // 计算下边界bin ID对应的bin数组索引
    let lower_bin_array_idx = BinArray::bin_id_to_bin_array_index(lower_bin_id)?;
    
    // 计算上边界bin ID对应的bin数组索引(通常是下边界索引+1)
    let upper_bin_array_idx = lower_bin_array_idx.checked_add(1).unwrap();

    // 获取流动性池地址
    let lb_pair = state.lb_pair;

    // 创建指令列表，首先添加计算预算指令（增加可用计算单元以支持复杂交易）
    let mut instructions = vec![ComputeBudgetInstruction::set_compute_unit_limit(1_400_000)];
    
    // 遍历所需的bin数组索引范围，确保它们都被初始化
    for idx in lower_bin_array_idx..=upper_bin_array_idx {
        // 派生bin数组的程序派生地址(PDA)
        let (bin_array, _bump) = derive_bin_array_pda(lb_pair, idx.into());

        // 检查bin数组是否已存在，如果不存在则添加初始化指令
        if program.rpc().get_account_data(&bin_array).is_err() {
            instructions.push(Instruction {
                program_id: lb_clmm::ID,
                accounts: accounts::InitializeBinArray {
                    bin_array,                                         // bin数组账户
                    funder: payer.pubkey(),                            // 支付创建账户费用的钱包
                    lb_pair,                                           // 流动性池账户
                    system_program: anchor_client::solana_sdk::system_program::ID, // 系统程序
                }
                .to_account_metas(None),
                data: instruction::InitializeBinArray { index: idx.into() }.data(), // 初始化数据，包含bin数组索引
            })
        }
    }

    // 创建新的仓位密钥对和公钥
    let position_kp = Keypair::new();
    let position = position_kp.pubkey();
    
    // 派生事件授权的程序派生地址，用于发出事件
    let (event_authority, _bump) =
        Pubkey::find_program_address(&[b"__event_authority"], &lb_clmm::ID);

    // 添加初始化仓位的指令
    instructions.push(Instruction {
        program_id: lb_clmm::ID,
        accounts: accounts::InitializePosition {
            lb_pair,                                         // 流动性池账户
            payer: payer.pubkey(),                           // 支付创建账户费用的钱包
            position,                                        // 要创建的仓位账户
            owner: payer.pubkey(),                           // 仓位所有者
            rent: anchor_client::solana_sdk::sysvar::rent::ID, // 租金系统变量
            system_program: anchor_client::solana_sdk::system_program::ID, // 系统程序
            event_authority,                                 // 事件授权账户
            program: lb_clmm::ID,                            // 程序自身账户
        }
        .to_account_metas(None),
        data: instruction::InitializePosition {
            lower_bin_id,                                    // 仓位的下边界bin ID
            width: MAX_BIN_PER_POSITION as i32,              // 仓位宽度（包含的bin数量）
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