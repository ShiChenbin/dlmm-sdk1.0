use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use anyhow::*;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub rpc_url: String,
    pub wallet_mnemonic: Option<String>, // 助记词，可选
    pub wallet_path: Option<String>,     // 文件路径，可选
    pub pool_address: String,
    pub slippage: f64,
    pub check_interval_seconds: u64,
    pub min_sol_reserve: f64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rpc_url: "https://api.mainnet-beta.solana.com".to_string(),
            wallet_mnemonic: None,
            wallet_path: None,
            pool_address: "5rCf1DM8LjKTw4YqhnoLcngyZYeNnQqztScTogYHAS6".to_string(),
            slippage: 0.1, // 10%
            check_interval_seconds: 10,
            min_sol_reserve: 0.2,
        }
    }
}

// 全局配置管理器
pub struct ConfigManager {
    config: Arc<Mutex<Config>>,
}

impl ConfigManager {
    pub fn new() -> Self {
        Self {
            config: Arc::new(Mutex::new(Config::default())),
        }
    }
    
    // 更新配置 - 从前端调用
    pub fn update_config(&self, new_config: Config) -> Result<()> {
        let mut config = self.config.lock().map_err(|_| Error::msg("锁定配置时出错"))?;
        *config = new_config;
        Ok(())
    }
    
    // 设置RPC URL
    pub fn set_rpc_url(&self, rpc_url: String) -> Result<()> {
        let mut config = self.config.lock().map_err(|_| Error::msg("锁定配置时出错"))?;
        config.rpc_url = rpc_url;
        Ok(())
    }
    
    // 设置钱包助记词
    pub fn set_wallet_mnemonic(&self, mnemonic: String) -> Result<()> {
        let mut config = self.config.lock().map_err(|_| Error::msg("锁定配置时出错"))?;
        config.wallet_mnemonic = Some(mnemonic);
        config.wallet_path = None; // 清除文件路径，优先使用助记词
        Ok(())
    }
    
    // 获取当前配置的克隆
    pub fn get_config(&self) -> Result<Config> {
        let config = self.config.lock().map_err(|_| Error::msg("锁定配置时出错"))?;
        Ok(config.clone())
    }
}