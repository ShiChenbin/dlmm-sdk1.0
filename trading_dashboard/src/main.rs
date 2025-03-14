mod wallet;
mod liquidity;
mod monitoring;
mod swap;
mod config;
mod types;
mod utils;
mod state;
mod pair_config;
mod bin_array_manager;

use config::ConfigManager;
use wallet::{WalletManager, get_sol_balance, get_token_balance};
use liquidity::LiquidityManager;
use monitoring::MonitoringManager;
use swap::SwapManager;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::io::{self, BufRead};
use tokio::time::interval;
use anyhow::*;
use env_logger;
use anchor_client::solana_sdk::pubkey::Pubkey;
use crate::state::{AllPosition, SinglePosition};
use std::str::FromStr;
use solana_client::rpc_request::TokenAccountsFilter;
use std::result::Result::{Ok, Err};

const MIN_SOL_RESERVE: f64 = 0.2; // 最小保留的SOL数量
const SLIPPAGE_RATE: f64 = 0.5; // 交易滑点 0.5%
const CHECK_INTERVAL_SECONDS: u64 = 10; // 价格区间检查间隔
const SOL_DECIMALS: u8 = 9;
const USDC_DECIMALS: u8 = 6;
const USDC_MINT: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"; // USDC代币的Mint地址

fn setup_logging() {
    let env = env_logger::Env::default().filter_or("LOG_LEVEL", "info");
    env_logger::Builder::from_env(env).init();
}

// 读取用户输入
fn read_user_input(prompt: &str) -> Result<String> {
    println!("{}", prompt);
    let mut line = String::new();
    let stdin = io::stdin();
    stdin.lock().read_line(&mut line)?;
    Ok(line.trim().to_string())
}

#[tokio::main]
async fn main() -> Result<()> {
    setup_logging();
    
    // 交易循环
    loop {
        // 步骤1: 输入钱包助记词，载入钱包
        let mnemonic = read_user_input("请输入钱包助记词:")?;
        
        // 创建配置管理器
        let config_manager = Arc::new(ConfigManager::new());
        config_manager.set_rpc_url("https://api.mainnet-beta.solana.com".to_string())?;
        config_manager.set_wallet_mnemonic(mnemonic)?;
        
        // 创建钱包管理器
        let wallet_manager = Arc::new(WalletManager::new(config_manager.config.clone()));
        wallet_manager.initialize_wallet()?;
        
        // 获取钱包地址并显示
        let wallet_address = wallet_manager.get_wallet_address()?;
        println!("已连接钱包: {}", wallet_address);
        
        // 步骤2: 查询并显示USDC和SOL余额
        let pubkey = wallet_manager.get_pubkey()?;
        let client = wallet_manager.create_rpc_client()?;
        
        // 查询SOL余额
        let sol_balance = get_sol_balance(&client, &pubkey).await?;
        println!("SOL余额: {} SOL", sol_balance);
        
        // 查询USDC余额
        let usdc_mint = Pubkey::from_str(USDC_MINT)?;
        let token_accounts = client.get_token_accounts_by_owner(
            &pubkey, 
            TokenAccountsFilter::Mint(usdc_mint))?;
        
        let mut usdc_balance: f64 = 0.0;
        if let Some(token_account) = token_accounts.first() {
            let token_account_pubkey = Pubkey::from_str(&token_account.pubkey)?;
            let usdc_amount = get_token_balance(&client, &token_account_pubkey).await?;
            usdc_balance = usdc_amount as f64 / 10f64.powf(USDC_DECIMALS as f64);
            println!("USDC余额: {} USDC", usdc_balance);
        } else {
            println!("未找到USDC代币账户");
        }
        
        // 步骤3: 输入要查询的流动性池子地址
        let pool_address = read_user_input("请输入要查询的流动性池地址 (默认: 5rCf1DM8LjKTw4YqhnoLcngyZYeNnQqztScTogYHAS6):")?;
        let pool_address = if pool_address.is_empty() {
            "5rCf1DM8LjKTw4YqhnoLcngyZYeNnQqztScTogYHAS6".to_string()
        } else {
            pool_address
        };
        
        // 修改配置
        {
            let mut config = config_manager.config.lock().map_err(|_| anyhow!("配置锁定失败"))?;
            config.pool_address = pool_address.clone();
            config.slippage = 10.0; // 10% 滑点
            config.check_interval_seconds = CHECK_INTERVAL_SECONDS;
            config.min_sol_reserve = MIN_SOL_RESERVE;
        }
        
        // 查询流动性池的24h fee/tvl
        println!("查询流动性池 {} 的24h fee/tvl...", pool_address);
        // 这里需要实现查询24h fee和tvl的逻辑
        // TODO: 实现API调用获取这些数据
        println!("24h Fee: 暂无数据");
        println!("TVL: 暂无数据");
        
        // 步骤4: 检查SOL余额并执行流动性操作
        if sol_balance >= MIN_SOL_RESERVE {
            println!("SOL余额足够，开始执行流动性操作...");
            
            // 获取配置
            let config = config_manager.get_config()?;
            let pair_config = pair_config::get_pair_config_for_pool(&config.pool_address)?;
            
            // 创建状态和管理器
            let state = Arc::new(Mutex::new(state::AllPosition::new(&vec![pair_config.clone()])));
            let liquidity_manager = Arc::new(LiquidityManager::new(wallet_manager.clone(), state.clone()));
            let swap_manager = Arc::new(SwapManager::new(wallet_manager.clone()));
            let monitoring_manager = Arc::new(MonitoringManager::new(
                vec![pair_config.clone()], 
                liquidity_manager.clone(), 
                swap_manager.clone()
            ));
            
            // 步骤4a: 计算要添加的流动性
            let sol_amount_to_add = (sol_balance - MIN_SOL_RESERVE) * 10f64.powf(SOL_DECIMALS as f64);
            let usdc_amount_to_add = usdc_balance * 10f64.powf(USDC_DECIMALS as f64);
            
            println!("添加流动性: {} SOL 和 {} USDC", sol_balance - MIN_SOL_RESERVE, usdc_balance);
            
            // 获取流动性池
            let lb_pair = Pubkey::from_str(&pool_address)?;
            let mut position = SinglePosition::new(lb_pair);
            
            // 刷新状态以获取最新的池状态
            // TODO: 实现刷新状态的逻辑
            println!("刷新池状态...");
            
            // 添加流动性
            liquidity_manager.add_liquidity(
                &position,
                sol_amount_to_add as u64,
                usdc_amount_to_add as u64,
                position.lb_pair_state.active_id
            ).await?;
            
            println!("已成功添加流动性");
            
            // 步骤4b: 每10秒检查价格区间
            println!("开始监控价格区间，每{}秒检查一次...", CHECK_INTERVAL_SECONDS);
            let mut check_interval = interval(Duration::from_secs(CHECK_INTERVAL_SECONDS));
            
            let mut removed_liquidity = false;
            while !removed_liquidity {
                check_interval.tick().await;
                println!("检查价格区间...");
                
                // 更新position状态
                // TODO: 实现更新状态的逻辑
                
                // 检查价格是否超出区间
                let price_outside = monitoring_manager.check_price_range(&position).await?;
                
                if price_outside {
                    println!("价格已超出区间，取出所有流动性...");
                    
                    // 步骤4c: 取出所有流动性
                    liquidity_manager.remove_liquidity(&position).await?;
                    removed_liquidity = true;
                    
                    println!("已成功取出所有流动性");
                    
                    // 步骤5: 使用Jupiter进行代币交换
                    println!("开始代币交换...");
                    
                    // 再次查询SOL和USDC余额
                    let sol_balance = get_sol_balance(&client, &pubkey).await?;
                    println!("当前SOL余额: {} SOL", sol_balance);
                    
                    let mut usdc_balance: f64 = 0.0;
                    if let Some(token_account) = token_accounts.first() {
                        let token_account_pubkey = Pubkey::from_str(&token_account.pubkey)?;
                        let usdc_amount = get_token_balance(&client, &token_account_pubkey).await?;
                        usdc_balance = usdc_amount as f64 / 10f64.powf(USDC_DECIMALS as f64);
                        println!("当前USDC余额: {} USDC", usdc_balance);
                    }
                    
                    // 计算要交换的金额
                    let sol_to_swap = (sol_balance / 2.0) * 10f64.powf(SOL_DECIMALS as f64);
                    let usdc_to_swap = (usdc_balance / 2.0) * 10f64.powf(USDC_DECIMALS as f64);
                    
                    // 交换SOL到USDC
                    println!("将 {} SOL 换成 USDC...", sol_balance / 2.0);
                    let max_retries = 3;
                    let swap_result = swap_manager.swap_with_retry(
                        &position, 
                        sol_to_swap as u64, 
                        false,  // swap_for_y = false 意味着从SOL兑换为USDC
                        max_retries
                    ).await;
                    
                    if let Ok(event) = swap_result {
                        println!("交换成功: 输入 {} SOL, 获得 {} USDC", 
                            event.amount_in as f64 / 10f64.powf(SOL_DECIMALS as f64),
                            event.amount_out as f64 / 10f64.powf(USDC_DECIMALS as f64));
                    } else if let Err(e) = swap_result {
                        println!("SOL到USDC交换失败: {}", e);
                    }
                    
                    // 交换USDC到SOL
                    println!("将 {} USDC 换成 SOL...", usdc_balance / 2.0);
                    let swap_result = swap_manager.swap_with_retry(
                        &position, 
                        usdc_to_swap as u64, 
                        true,  // swap_for_y = true 意味着从USDC兑换为SOL
                        max_retries
                    ).await;
                    
                    if let Ok(event) = swap_result {
                        println!("交换成功: 输入 {} USDC, 获得 {} SOL", 
                            event.amount_in as f64 / 10f64.powf(USDC_DECIMALS as f64),
                            event.amount_out as f64 / 10f64.powf(SOL_DECIMALS as f64));
                    } else if let Err(e) = swap_result {
                        println!("USDC到SOL交换失败: {}", e);
                    }
                    
                    println!("代币交换完成，进入新的交易循环...");
                }
            }
        } else {
            println!("SOL余额不足 {} SOL，无法执行流动性操作", MIN_SOL_RESERVE);
        }
        
        println!("\n交易循环完成，开始新的循环...\n");
    }
}