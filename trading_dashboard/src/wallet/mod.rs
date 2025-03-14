mod balance;
mod reader;

pub use balance::*;
pub use reader::*;

use crate::config::Config;
use anchor_client::{Cluster, Client, Program};
use anchor_client::solana_sdk::signature::Keypair;
use anchor_client::solana_sdk::signature::Signer;
use anchor_client::solana_sdk::pubkey::Pubkey;
use anchor_client::solana_sdk::commitment_config::CommitmentConfig;
use std::sync::{Arc, Mutex};
use anyhow::*;

pub struct WalletManager {
    pub config: Arc<Mutex<Config>>,
    keypair: Arc<Mutex<Option<Keypair>>>,
}

impl WalletManager {
    pub fn new(config: Arc<Mutex<Config>>) -> Self {
        Self { 
            config,
            keypair: Arc::new(Mutex::new(None)),
        }
    }
    
    // 初始化钱包
    pub fn initialize_wallet(&self) -> Result<()> {
        let config = self.config.lock().map_err(|_| Error::msg("配置锁定失败"))?;
        
        let keypair = reader::read_wallet(&config.wallet_path, &config.wallet_mnemonic)?;
        
        // 更新keypair
        let mut keypair_lock = self.keypair.lock().map_err(|_| Error::msg("钱包锁定失败"))?;
        *keypair_lock = Some(keypair);
        
        Ok(())
    }
    
    // 获取当前钱包公钥地址
    pub fn get_wallet_address(&self) -> Result<String> {
        let keypair_lock = self.keypair.lock().map_err(|_| Error::msg("钱包锁定失败"))?;
        
        if let Some(ref keypair) = *keypair_lock {
            Ok(keypair.pubkey().to_string())
        } else {
            Err(Error::msg("尚未初始化钱包"))
        }
    }
    
    // 获取钱包的Pubkey
    pub fn get_pubkey(&self) -> Result<Pubkey> {
        let keypair_lock = self.keypair.lock().map_err(|_| Error::msg("钱包锁定失败"))?;
        
        if let Some(ref keypair) = *keypair_lock {
            Ok(keypair.pubkey())
        } else {
            Err(Error::msg("尚未初始化钱包"))
        }
    }
    
    // 获取钱包的克隆
    pub fn get_keypair(&self) -> Result<Keypair> {
        let keypair_lock = self.keypair.lock().map_err(|_| Error::msg("钱包锁定失败"))?;
        
        if let Some(ref keypair) = *keypair_lock {
            Ok(keypair.insecure_clone())
        } else {
            Err(Error::msg("尚未初始化钱包"))
        }
    }
    
    // 为特定程序创建Program客户端
    pub fn create_program<T>(&self, program_id: T) -> Result<Program<Arc<Keypair>>> 
    where 
        T: Into<Pubkey>
    {
        let payer = self.get_keypair()?;
        let cluster = self.get_cluster()?;
        
        let client = anchor_client::Client::new_with_options(
            cluster,
            Arc::new(payer),
            CommitmentConfig::processed(),
        );
        
        Ok(client.program(program_id.into())
            .map_err(|e| anyhow!("创建程序客户端失败: {}", e))?)
    }
    
    // 创建RPC客户端
    pub fn create_rpc_client(&self) -> Result<solana_client::rpc_client::RpcClient> {
        let config = self.config.lock().map_err(|_| anyhow!("配置锁定失败"))?;
        Ok(solana_client::rpc_client::RpcClient::new(config.rpc_url.clone()))
    }

    // 获取集群配置
    fn get_cluster(&self) -> Result<anchor_client::Cluster> {
        let config = self.config.lock().map_err(|_| anyhow!("配置锁定失败"))?;
        
        Ok(match config.rpc_url.as_str() {
            "https://api.mainnet-beta.solana.com" => anchor_client::Cluster::Mainnet,
            "https://api.devnet.solana.com" => anchor_client::Cluster::Devnet,
            _ => anchor_client::Cluster::Custom(config.rpc_url.clone(), config.rpc_url.clone()),
        })
    }
}