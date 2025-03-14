use trading_dashboard::*;
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn test_full_flow() -> anyhow::Result<()> {
    // 创建配置管理器
    let config_manager = Arc::new(ConfigManager::new());
    
    // 使用devnet进行测试
    config_manager.set_rpc_url("https://api.devnet.solana.com".to_string())?;
    config_manager.set_wallet_mnemonic("这里填入你的测试助记词".to_string())?;
    
    // 初始化各组件
    let wallet_manager = Arc::new(WalletManager::new(config_manager.config.clone()));
    wallet_manager.initialize_wallet()?;
    
    // 验证钱包地址
    let wallet_address = wallet_manager.get_wallet_address()?;
    assert!(!wallet_address.is_empty());
    
    // 其它测试逻辑...
    
    Ok(())
} 