use crate::config::{Config, ConfigManager};
use crate::wallet::WalletManager;
use warp::{Filter, Rejection, Reply};
use std::sync::Arc;
use anyhow::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct ConnectRequest {
    pub rpc_url: String,
    pub mnemonic: String,
}

#[derive(Debug, Serialize)]
pub struct ConnectResponse {
    pub success: bool,
    pub wallet_address: Option<String>,
    pub error: Option<String>,
}

pub async fn start_api_server(
    config_manager: Arc<ConfigManager>,
    wallet_manager: Arc<WalletManager>,
    port: u16,
) {
    // 连接钱包API
    let connect = warp::path("connect")
        .and(warp::post())
        .and(warp::body::json())
        .and(with_config_manager(config_manager.clone()))
        .and(with_wallet_manager(wallet_manager.clone()))
        .and_then(handle_connect);
    
    // 启动服务器
    let api = connect;
    
    println!("API服务器启动于端口 {}", port);
    warp::serve(api).run(([0, 0, 0, 0], port)).await;
}

// 处理连接请求
async fn handle_connect(
    req: ConnectRequest, 
    config_manager: Arc<ConfigManager>,
    wallet_manager: Arc<WalletManager>,
) -> Result<impl Reply, Rejection> {
    // 更新配置
    if let Err(e) = config_manager.set_rpc_url(req.rpc_url) {
        return Ok(warp::reply::json(&ConnectResponse {
            success: false,
            wallet_address: None,
            error: Some(format!("设置RPC URL失败: {}", e)),
        }));
    }
    
    if let Err(e) = config_manager.set_wallet_mnemonic(req.mnemonic) {
        return Ok(warp::reply::json(&ConnectResponse {
            success: false,
            wallet_address: None,
            error: Some(format!("设置助记词失败: {}", e)),
        }));
    }
    
    // 初始化钱包
    if let Err(e) = wallet_manager.initialize_wallet() {
        return Ok(warp::reply::json(&ConnectResponse {
            success: false,
            wallet_address: None,
            error: Some(format!("初始化钱包失败: {}", e)),
        }));
    }
    
    // 获取钱包地址
    match wallet_manager.get_wallet_address() {
        Ok(address) => {
            Ok(warp::reply::json(&ConnectResponse {
                success: true,
                wallet_address: Some(address),
                error: None,
            }))
        },
        Err(e) => {
            Ok(warp::reply::json(&ConnectResponse {
                success: false,
                wallet_address: None,
                error: Some(format!("获取钱包地址失败: {}", e)),
            }))
        }
    }
}

// 辅助函数
fn with_config_manager(
    config_manager: Arc<ConfigManager>,
) -> impl Filter<Extract = (Arc<ConfigManager>,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || config_manager.clone())
}

fn with_wallet_manager(
    wallet_manager: Arc<WalletManager>,
) -> impl Filter<Extract = (Arc<WalletManager>,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || wallet_manager.clone())
}