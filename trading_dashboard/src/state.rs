use anchor_lang::prelude::Pubkey;
use anchor_spl::token::Mint;
use lb_clmm::state::{bin::BinArray, lb_pair::LbPair, position::PositionV2};
use std::collections::HashMap;
use std::str::FromStr;
use crate::pair_config::PairConfig;

// 项目全局状态，管理所有仓位
pub struct AllPosition {
    pub all_positions: HashMap<Pubkey, SinglePosition>, // hashmap of pool pubkey and a position
    pub tokens: HashMap<Pubkey, Mint>,                  // cached token info
}

impl AllPosition {
    pub fn new(config: &Vec<PairConfig>) -> Self {
        let mut all_positions = HashMap::new();
        for pair in config.iter() {
            let pool_pk = Pubkey::from_str(&pair.pair_address).unwrap();
            all_positions.insert(pool_pk, SinglePosition::new(pool_pk));
        }
        AllPosition {
            all_positions,
            tokens: HashMap::new(),
        }
    }
}

// 单个池子的状态
#[derive(Default, Debug, Clone)]
pub struct SinglePosition {
    pub lb_pair: Pubkey,
    pub lb_pair_state: LbPair,
    pub bin_arrays: HashMap<Pubkey, BinArray>, // only store relevant bin arrays
    pub positions: Vec<PositionV2>,
    pub position_pks: Vec<Pubkey>,
    pub rebalance_time: u64,
    pub min_bin_id: i32,
    pub max_bin_id: i32,
    pub last_update_timestamp: u64,
}

impl SinglePosition {
    pub fn new(lb_pair: Pubkey) -> Self {
        SinglePosition {
            lb_pair,
            rebalance_time: 0,
            lb_pair_state: LbPair::default(),
            bin_arrays: HashMap::new(),
            positions: vec![],
            position_pks: vec![],
            min_bin_id: 0,
            max_bin_id: 0,
            last_update_timestamp: 0,
        }
    }
    
    pub fn inc_rebalance_time(&mut self) {
        self.rebalance_time += 1;
    }
}

// 辅助函数
pub fn get_decimals(mint: Pubkey, tokens: &HashMap<Pubkey, u8>) -> u8 {
    // 尝试从tokens映射中获取小数位数
    tokens.get(&mint).copied().unwrap_or(6) // 默认为6位小数
} 