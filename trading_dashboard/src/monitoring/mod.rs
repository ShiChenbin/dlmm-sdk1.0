mod price_range;
mod metrics;

pub use price_range::*;
pub use metrics::*;

use crate::pair_config::{get_pair_config, PairConfig};
use crate::state::SinglePosition;
use crate::MarketMakingMode;
use anyhow::*;

pub struct MonitoringManager {
    pub config: Vec<PairConfig>,
    pub liquidity_manager: LiquidityManager,
    pub swap_manager: SwapManager,
}

impl MonitoringManager {
    pub fn new(config: Vec<PairConfig>, liquidity_manager: LiquidityManager, swap_manager: SwapManager) -> Self {
        Self { config, liquidity_manager, swap_manager }
    }

    // 价格区间监控(从Core.check_shift_price_range迁移)
    pub async fn check_price_range(&self, position: &SinglePosition) -> Result<bool> {
        let pair_config = get_pair_config(&self.config, position.lb_pair);
        
        // 检查价格是否超出区间
        match pair_config.mode {
            MarketMakingMode::ModeRight if position.lb_pair_state.active_id > position.max_bin_id => {
                self.shift_right(position).await?;
                self.inc_rebalance_time(position.lb_pair);
                return Ok(true);
            },
            MarketMakingMode::ModeLeft if position.lb_pair_state.active_id < position.min_bin_id => {
                self.shift_left(position).await?;
                self.inc_rebalance_time(position.lb_pair);
                return Ok(true);
            },
            MarketMakingMode::ModeBoth => {
                if position.lb_pair_state.active_id < position.min_bin_id {
                    self.shift_left(position).await?;
                    self.inc_rebalance_time(position.lb_pair);
                    return Ok(true);
                } else if position.lb_pair_state.active_id > position.max_bin_id {
                    self.shift_right(position).await?;
                    self.inc_rebalance_time(position.lb_pair);
                    return Ok(true);
                }
            },
            _ => {}
        }
        
        Ok(false)
    }
    
    // private methods for shifting positions...
}