mod wallet;
mod liquidity;
mod monitoring;
mod swap;
mod config;
mod types;
mod utils;
mod state;
mod pair_config;

use config::ConfigManager;
use wallet::WalletManager;
use liquidity::LiquidityManager;
use monitoring::MonitoringManager;
use swap::SwapManager;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::interval;
use anyhow::*;

#[tokio::main]
async fn main() -> Result<()> {
    // 创建配置管理器
    let config_manager = Arc::new(ConfigManager::new());
    
    // 创建钱包管理器
    let wallet_manager = Arc::new(WalletManager::new(config_manager.config.clone()));
    
    // 等待用户从前端输入RPC和钱包信息
    println!("等待配置信息...");
    
    // 这里模拟用户输入 - 在真实应用中这会从API/UI获取
    config_manager.set_rpc_url("https://api.mainnet-beta.solana.com".to_string())?;
    config_manager.set_wallet_mnemonic("your twelve word mnemonic phrase here ...".to_string())?;
    
    // 初始化钱包
    wallet_manager.initialize_wallet()?;
    
    // 获取钱包地址
    let wallet_address = wallet_manager.get_wallet_address()?;
    println!("已连接钱包: {}", wallet_address);
    
    // 初始化状态和其他管理器
    let config = config_manager.get_config()?;
    let pair_config = pair_config::get_pair_config_for_pool(&config.pool_address)?;
    let state = Arc::new(Mutex::new(state::AllPosition::new(&vec![pair_config.clone()])));
    
    let liquidity_manager = Arc::new(LiquidityManager::new(wallet_manager.clone(), state.clone()));
    let swap_manager = Arc::new(SwapManager::new(wallet_manager.clone()));
    let monitoring_manager = Arc::new(MonitoringManager::new(
        vec![pair_config], 
        liquidity_manager.clone(), 
        swap_manager.clone()
    ));
    
    // 开始主循环
    let mut interval = interval(Duration::from_secs(config.check_interval_seconds));
    
    loop {
        interval.tick().await;
        println!("检查价格区间...");
        
        // 执行策略...
    }
}