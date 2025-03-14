use anchor_client::Cluster;
use anchor_lang::prelude::Pubkey;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum MarketMakingMode {
    ModeRight,
    ModeLeft,
    ModeBoth,
}

// 其他通用数据类型...