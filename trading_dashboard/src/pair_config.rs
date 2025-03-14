use anchor_client::solana_sdk::pubkey::Pubkey;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use anyhow::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MarketMakingMode {
    ModeRight,   // 价格上涨趋势
    ModeLeft,    // 价格下跌趋势
    ModeBoth,    // 双向波动
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairConfig {
    pub pair_address: String,
    pub token_x_mint: String,
    pub token_y_mint: String,
    pub x_amount: u64,
    pub y_amount: u64,
    pub mode: MarketMakingMode,
}

// 获取特定池子的配置
pub fn get_pair_config(configs: &Vec<PairConfig>, lb_pair: Pubkey) -> PairConfig {
    // 查找池子配置
    for config in configs {
        if let Ok(pubkey) = Pubkey::from_str(&config.pair_address) {
            if pubkey == lb_pair {
                return config.clone();
            }
        }
    }
    
    // 未找到则返回默认配置
    PairConfig {
        pair_address: lb_pair.to_string(),
        token_x_mint: "".to_string(),
        token_y_mint: "".to_string(),
        x_amount: 1000,
        y_amount: 1000,
        mode: MarketMakingMode::ModeBoth,
    }
}

// 根据池子地址获取配置
pub fn get_pair_config_for_pool(pool_address: &str) -> Result<PairConfig> {
    let default_config = PairConfig {
        pair_address: pool_address.to_string(),
        token_x_mint: "So11111111111111111111111111111111111111112".to_string(), // SOL代币地址
        token_y_mint: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(), // USDC代币地址
        x_amount: 1_000_000, // 默认值，单位为lamports
        y_amount: 1_000_000, // 默认值，单位为最小单位
        mode: MarketMakingMode::ModeBoth,
    };
    
    Ok(default_config)
} 