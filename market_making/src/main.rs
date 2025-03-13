pub mod bin_array_manager;
pub mod core;
pub mod pair_config;
pub mod router;
pub mod state;
pub mod utils;
use anchor_client::solana_sdk::pubkey::Pubkey;
use anchor_client::solana_sdk::signature::read_keypair_file;
use anchor_client::solana_sdk::signer::Signer;
use anchor_client::Cluster;
use clap::Parser;
use core::Core;
use hyper::Server;
use pair_config::{get_config_from_file, should_market_making};
use router::router;
use routerify::RouterService;
use serde::{Deserialize, Serialize};
use state::AllPosition;
use std::convert::Into;
use std::fmt::Debug;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

#[macro_use]
extern crate log;

use tokio::time::interval;

/**
 * 原代码为做市商策略 
 */


#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
/// 做市策略：1.右侧池做市 2.左侧池做市 3.双边池做市 4.查看模式
pub enum MarketMakingMode {
    ModeRight,
    ModeLeft,
    ModeBoth,
    ModeView,
}

impl Default for MarketMakingMode {
    fn default() -> Self {
        MarketMakingMode::ModeView
    }
}
// impl fmt::Display for MarketMakingMode {
//     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//         write!(f, "{}", MarketMakingMode::ModeRight)
//     }
// }

impl FromStr for MarketMakingMode {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "moderight" => Ok(MarketMakingMode::ModeRight),
            "modeleft" => Ok(MarketMakingMode::ModeLeft),
            "modeboth" => Ok(MarketMakingMode::ModeBoth),
            "modeview" => Ok(MarketMakingMode::ModeView),
            _ => Err(anyhow::Error::msg("cannot get mode")),
        }
    }
}

#[derive(Parser, Debug)]
pub struct Args {
    /// Solana RPC provider. For example: https://api.mainnet-beta.solana.com
    #[clap(long, default_value_t = Cluster::Localnet)]
    provider: Cluster,  // Solana RPC提供者
    /// Wallet of owner
    #[clap(long)]
    wallet: Option<String>,  // 所有者钱包路径
    /// Address of owner, only user_public_key or wallet is set, other wise it is panic immediately
    #[clap(long)]
    user_public_key: Option<Pubkey>,  // 所有者公钥地址
    /// config path
    #[clap(long)]
    config_file: String,  // 配置文件路径
    // /// public key pair address,
    // #[clap(long)]
    // pair_address: Pubkey,
    // /// market making mode, Ex: mr: mode right
    // #[clap(long)]
    // mode: MarketMakingMode,
    // // min amount for mm
    // #[clap(long)]
    // min_x_amount: u64,
    // #[clap(long)]
    // min_y_amount: u64,
}

#[tokio::main(worker_threads = 20)] // TODO figure out why it is blocking in linux
async fn main() {
    env_logger::init();

    // 不再从命令行解析参数，而是直接硬编码
    // let Args {
    //     provider,
    //     wallet,
    //     user_public_key,
    //     config_file,
    // } = Args::parse();
    //
    // 硬编码参数 https://api.devnet.solana.com
    let provider = Cluster::Custom(
        "https://mainnet.helius-rpc.com/?api-key=f17bedeb-4c13-407b-9b80-f4a6f4599fe3".to_string(),
        "wss://mainnet.helius-rpc.com/?api-key=f17bedeb-4c13-407b-9b80-f4a6f4599fe3".to_string() // WebSocket URL 先随便塞一个，可能无法使用
    ); // 使用Helius RPC提供商
    let wallet = Some(String::from("./src/wallet_keypair.json")); // 
    // 使用cli 创建的钱包的公钥 VWdHkVXCbxmUBxu6pQHpkmooR8Dvh8LdzCCKz2WHGNv
    let user_public_key = Some(Pubkey::from_str("HFFajb363qWRuKbPhztjzMBM1b4TBfxJSq32EgySSGxB").unwrap()); // 如果需要，可以用 Some(Pubkey::from_str("你的公钥").unwrap())
    let config_file = String::from("./src/config.json"); // 替换为实际配置文件路径

    println!("正在加载配置文件: {}", config_file);
    let config = match get_config_from_file(&config_file) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("加载配置文件失败: {}", e);
            panic!("无法加载配置文件，程序终止");
        }
    };
    println!("配置文件加载成功");

    // info!("{:?}", mode);

    // 在使用钱包文件之前添加验证
    if let Some(wallet_path) = wallet.as_ref() {
        println!("验证钱包文件...");
        match read_keypair_file(wallet_path) {
            Ok(keypair) => {
                let wallet_pubkey = keypair.pubkey().to_string();
                let expected_pubkey = user_public_key.unwrap().to_string();
                println!("钱包文件公钥: {}", wallet_pubkey);
                println!("预期公钥: {}", expected_pubkey);
                
                if wallet_pubkey != expected_pubkey {
                    eprintln!("警告：钱包文件公钥与预期公钥不匹配！");
                } else {
                    println!("钱包文件验证成功，公钥匹配");
                }
            },
            Err(e) => {
                eprintln!("无法读取钱包文件：{}", e);
            }
        }
    }

    let user_wallet = if should_market_making(&config) {
        let wallet =
            read_keypair_file(wallet.clone().unwrap()).expect("Wallet keypair file not found");
        wallet.pubkey()
    } else {
        user_public_key.unwrap()
    };
    // 声明 core 以调用core函数
    let core = Core {
        provider,
        wallet,
        owner: user_wallet,
        config: config.clone(),
        state: Arc::new(Mutex::new(AllPosition::new(&config))),
    };

    // init some stat
    core.refresh_state().await.unwrap();
    core.fetch_token_info().unwrap();
    let core = Arc::new(core);
    let mut handles = vec![];
    {
        // crawl epoch down
        let core = core.clone();
        let handle = tokio::spawn(async move {
            let duration = 60; // 1 min
            let mut interval = interval(Duration::from_secs(duration));
            loop {
                interval.tick().await;
                info!("refresh state");
                match core.refresh_state().await {
                    Ok(_) => {}
                    Err(err) => error!("refresh_state err {}", err),
                }
            }
        });
        handles.push(handle);
    }

    if should_market_making(&config) {
        {
            // crawl epoch down
            let core = core.clone();

            // init user ata
            core.init_user_ata().await.unwrap();

            let handle = tokio::spawn(async move {
                let duration = 60; // 1 min
                let mut interval = interval(Duration::from_secs(duration));
                loop {
                    interval.tick().await;
                    info!("check shift price range");
                    match core.check_shift_price_range().await {
                        Ok(_) => {}
                        Err(err) => error!("check shift price err {}", err),
                    }
                }
            });
            handles.push(handle);
        }
    }

    // let mut handles = vec![];

    let router = router(core);

    let service = RouterService::new(router).unwrap();

    let addr = ([0, 0, 0, 0], 8080).into();

    let server = Server::bind(&addr).serve(service);

    server.await.unwrap();

    for handle in handles {
        handle.await.unwrap();
    }
}
