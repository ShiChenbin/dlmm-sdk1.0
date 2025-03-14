use trading_dashboard::*;
use std::sync::{Arc, Mutex};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 创建配置管理器
    let config_manager = Arc::new(ConfigManager::new());
    
    // 设置测试RPC和测试助记词
    config_manager.set_rpc_url("https://api.devnet.solana.com".to_string())?;
    config_manager.set_wallet_mnemonic("这里填入你的测试助记词".to_string())?;
    
    // 创建钱包管理器
    let wallet_manager = Arc::new(WalletManager::new(config_manager.config.clone()));
    
    // 初始化钱包
    wallet_manager.initialize_wallet()?;
    
    // 获取钱包地址
    let wallet_address = wallet_manager.get_wallet_address()?;
    println!("已连接钱包: {}", wallet_address);
    
    // 创建状态
    let pair_config = vec![PairConfig {
        pair_address: "测试池子地址".to_string(),
        token_x_mint: "代币X地址".to_string(),
        token_y_mint: "代币Y地址".to_string(),
        x_amount: 1000,
        y_amount: 1000,
        mode: MarketMakingMode::ModeBoth,
    }];
    
    let state = Arc::new(Mutex::new(state::AllPosition::new(&pair_config)));
    
    // 创建流动性管理器
    let liquidity_manager = Arc::new(LiquidityManager::new(wallet_manager.clone(), state.clone()));
    
    // 创建交易管理器
    let swap_manager = Arc::new(SwapManager::new(wallet_manager.clone()));
    
    println!("初始化成功！");
    
    Ok(())
} 