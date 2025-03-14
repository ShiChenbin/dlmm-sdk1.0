use lb_clmm::state::{bin::BinArray, lb_pair::LbPair, position::PositionV2};
use anchor_client::{
    Client, Cluster,
    solana_sdk::{
        commitment_config::CommitmentConfig,
        pubkey::Pubkey,
        signature::{Keypair, read_keypair_file},
    },
};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::interval;

// 池子管理器结构体
pub struct PoolManager {
    client: Client<Arc<Keypair>>,
    pools: Arc<Mutex<Vec<LbPair>>>,
    positions: Arc<Mutex<Vec<PositionV2>>>,
}

impl PoolManager {
    // 创建池子管理器
    pub fn new(rpc_url: &str) -> Self {
        let cluster = Cluster::Custom(rpc_url.to_string(), rpc_url.to_string());
        // 使用一个空keypair，因为只需要读取数据
        let payer = Arc::new(Keypair::new());
        let client = Client::new_with_options(
            cluster,
            payer,
            CommitmentConfig::confirmed(),
        );

        PoolManager {
            client,
            pools: Arc::new(Mutex::new(Vec::new())),
            positions: Arc::new(Mutex::new(Vec::new())),
        }
    }

    // 获取特定池子数据
    pub async fn fetch_pool(&self, pool_address: &str) -> Option<LbPair> {
        let pool_pubkey = match Pubkey::from_str(pool_address) {
            Ok(pubkey) => pubkey,
            Err(_) => {
                println!("无效的池子地址");
                return None;
            }
        };

        // 使用anchor_client获取账户数据
        let program_id = Pubkey::from_str("LBUZKhRxPF3XoTz6gg68i68QvYYJJVi8B2LjVM1zBUx").unwrap();
        let program = self.client.program(program_id);
        
        match program.account::<LbPair>(pool_pubkey) {
            Ok(pool) => Some(pool),
            Err(err) => {
                println!("获取池子数据失败: {}", err);
                None
            }
        }
    }

    // 获取所有池子数据
    pub async fn fetch_all_pools(&self, pool_addresses: &[&str]) {
        let mut pools = Vec::new();
        
        for &address in pool_addresses {
            if let Some(pool) = self.fetch_pool(address).await {
                pools.push(pool);
            }
        }
        
        // 更新内部状态
        let mut state = self.pools.lock().unwrap();
        *state = pools;
        println!("已更新 {} 个池子的数据", state.len());
    }

    // 获取池子的仓位数据
    pub async fn fetch_positions(&self, owner_address: &str) {
        let owner_pubkey = match Pubkey::from_str(owner_address) {
            Ok(pubkey) => pubkey,
            Err(_) => {
                println!("无效的所有者地址");
                return;
            }
        };

        // 获取该所有者的所有仓位
        // 实际实现需要查询筛选出所有者的仓位
        let program_id = Pubkey::from_str("LBUZKhRxPF3XoTz6gg68i68QvYYJJVi8B2LjVM1zBUx").unwrap();
        let program = self.client.program(program_id);
        
        // 这里简化实现，实际需要根据程序的存储方式来获取
        let mut positions = Vec::new();

        // 打印当前加载的仓位数量
        let mut state = self.positions.lock().unwrap();
        *state = positions;
        println!("已加载 {} 个仓位", state.len());
    }

    // 获取所有池子
    pub fn get_pools(&self) -> Vec<LbPair> {
        let state = self.pools.lock().unwrap();
        state.clone()
    }

    // 按代币对筛选池子
    pub fn get_pool_by_tokens(&self, token_x: &str, token_y: &str) -> Option<LbPair> {
        let state = self.pools.lock().unwrap();
        
        state.iter().find(|pool| {
            // 将Pubkey转为字符串进行简单匹配
            let x_str = pool.token_x_mint.to_string();
            let y_str = pool.token_y_mint.to_string();
            x_str.contains(token_x) && y_str.contains(token_y)
        }).cloned()
    }
}

#[tokio::main]
async fn main() {
    println!("交易池数据服务启动！");
    
    // 创建池子管理器
    let pool_manager = PoolManager::new("https://api.mainnet-beta.solana.com");
    
    // 示例池子地址 (这些是假地址，需要替换为实际地址)
    let pool_addresses = [
        "ETH_USDT_POOL_ADDRESS",
        "BTC_USDT_POOL_ADDRESS",
        "SOL_USDC_POOL_ADDRESS",
    ];
    
    // 获取池子数据
    pool_manager.fetch_all_pools(&pool_addresses).await;
    
    // 获取某钱包的仓位
    pool_manager.fetch_positions("WALLET_ADDRESS").await;
    
    // 打印池子信息
    let pools = pool_manager.get_pools();
    println!("总共有 {} 个交易池", pools.len());
    
    for pool in &pools {
        println!(
            "池子: {}-{}, 活跃ID: {}, bin步长: {}",
            pool.token_x_mint, pool.token_y_mint, pool.active_id, pool.bin_step
        );
    }
    
    // 查询特定池子
    if let Some(pool) = pool_manager.get_pool_by_tokens("BTC", "USDT") {
        println!("\n查询特定池子详情:");
        println!("代币X: {}", pool.token_x_mint);
        println!("代币Y: {}", pool.token_y_mint);
        println!("活跃ID: {}", pool.active_id);
        println!("bin步长: {}", pool.bin_step);
        println!("状态: {}", pool.status);
    }
    
    // 定期刷新池子数据
    let pool_manager = Arc::new(pool_manager);
    let pm = pool_manager.clone();
    let pool_addrs = pool_addresses.to_vec();
    
    tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            println!("刷新池子数据...");
            pm.fetch_all_pools(&pool_addrs).await;
        }
    });
    
    // 保持主线程运行
    tokio::signal::ctrl_c().await.unwrap();
    println!("程序退出");
}
