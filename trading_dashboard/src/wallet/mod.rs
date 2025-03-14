mod balance;
mod reader;

pub use balance::*;
pub use reader::*;

use crate::config::Config;
use anchor_client::{Cluster, Client};
use anchor_client::solana_sdk::signature::Keypair;
use std::sync::{Arc, Mutex};
use anyhow::*;

pub struct WalletManager {
    pub config: Arc<Mutex<Config>>,
    pub keypair: Arc<Mutex<Option<Keypair>>>,
    pub client: Arc<Mutex<Option<Client<Arc<Keypair>>>>>,
}

impl WalletManager {
    pub fn new(config: Arc<Mutex<Config>>) -> Self {
        Self { 
            config,
            keypair: Arc::new(Mutex::new(None)),
            client: Arc::new(Mutex::new(None)),
        }
    }
    
    // 初始化或更新钱包
    pub fn initialize_wallet(&self) -> Result<()> {
        let config = self.config.lock().map_err(|_| Error::msg("锁定配置时出错"))?;
        
        let keypair = reader::read_wallet(&config.wallet_path, &config.wallet_mnemonic)?;
        
        // 更新keypair
        let mut keypair_lock = self.keypair.lock().map_err(|_| Error::msg("锁定钱包时出错"))?;
        *keypair_lock = Some(keypair);
        
        // 更新client
        self.initialize_client()?;
        
        Ok(())
    }
    
    // 初始化或更新客户端
    pub fn initialize_client(&self) -> Result<()> {
        let config = self.config.lock().map_err(|_| Error::msg("锁定配置时出错"))?;
        let keypair_lock = self.keypair.lock().map_err(|_| Error::msg("锁定钱包时出错"))?;
        
        if let Some(ref keypair) = *keypair_lock {
            let cluster = Cluster::Custom(
                config.rpc_url.clone(), 
                config.rpc_url.clone()
            );
            
            let keypair_arc = Arc::new(keypair.clone());
            let client = Client::new_with_options(
                cluster,
                keypair_arc,
                anchor_client::solana_sdk::commitment_config::CommitmentConfig::confirmed(),
            );
            
            let mut client_lock = self.client.lock().map_err(|_| Error::msg("锁定客户端时出错"))?;
            *client_lock = Some(client);
            
            Ok(())
        } else {
            Err(Error::msg("尚未初始化钱包"))
        }
    }
    
    // 获取当前钱包公钥地址
    pub fn get_wallet_address(&self) -> Result<String> {
        let keypair_lock = self.keypair.lock().map_err(|_| Error::msg("锁定钱包时出错"))?;
        
        if let Some(ref keypair) = *keypair_lock {
            Ok(keypair.pubkey().to_string())
        } else {
            Err(Error::msg("尚未初始化钱包"))
        }
    }
    
    // 获取client
    pub fn get_client(&self) -> Result<Client<Arc<Keypair>>> {
        let client_lock = self.client.lock().map_err(|_| Error::msg("锁定客户端时出错"))?;
        
        if let Some(ref client) = *client_lock {
            Ok(client.clone())
        } else {
            Err(Error::msg("尚未初始化客户端"))
        }
    }
}