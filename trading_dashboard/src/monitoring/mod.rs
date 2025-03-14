mod price_range;
mod metrics;

pub use price_range::*;
pub use metrics::*;

use crate::pair_config::{get_pair_config, PairConfig};
use crate::state::SinglePosition;
use crate::liquidity::LiquidityManager;
use crate::swap::SwapManager;
use crate::pair_config::MarketMakingMode;
use anyhow::*;
use lb_clmm::math::safe_math::SafeMath;
use std::sync::Arc;
use log::info;

pub struct MonitoringManager {
    pub config: Vec<PairConfig>,
    pub liquidity_manager: Arc<LiquidityManager>,
    pub swap_manager: Arc<SwapManager>,
}

impl MonitoringManager {
    pub fn new(config: Vec<PairConfig>, liquidity_manager: Arc<LiquidityManager>, swap_manager: Arc<SwapManager>) -> Self {
        Self { config, liquidity_manager, swap_manager }
    }

    // 价格区间监控
    pub async fn check_price_range(&self, position: &SinglePosition) -> Result<bool> {
        let pair_config = get_pair_config(&self.config, position.lb_pair);
        
        // 检查价格是否超出区间
        match pair_config.mode {
            MarketMakingMode::ModeRight if position.lb_pair_state.active_id > position.max_bin_id => {
                self.shift_right(position).await?;
                self.liquidity_manager.position_manager.inc_rebalance_time(position.lb_pair);
                return Ok(true);
            },
            MarketMakingMode::ModeLeft if position.lb_pair_state.active_id < position.min_bin_id => {
                self.shift_left(position).await?;
                self.liquidity_manager.position_manager.inc_rebalance_time(position.lb_pair);
                return Ok(true);
            },
            MarketMakingMode::ModeBoth => {
                if position.lb_pair_state.active_id < position.min_bin_id {
                    self.shift_left(position).await?;
                    self.liquidity_manager.position_manager.inc_rebalance_time(position.lb_pair);
                    return Ok(true);
                } else if position.lb_pair_state.active_id > position.max_bin_id {
                    self.shift_right(position).await?;
                    self.liquidity_manager.position_manager.inc_rebalance_time(position.lb_pair);
                    return Ok(true);
                }
            },
            _ => {}
        }
        
        Ok(false)
    }
    
    // 向右调整仓位 (价格上涨时)
    async fn shift_right(&self, state: &SinglePosition) -> Result<()> {
        let pair_config = get_pair_config(&self.config, state.lb_pair);
        info!("向右调整仓位 {}", state.lb_pair);
        
        // 获取当前仓位状态
        let position = state.get_positions()?;
        if position.amount_x != 0 {
            return Err(Error::msg("X代币数量不为零，不能向右调整"));
        }

        // 取出流动性
        info!("提取流动性 {}", state.lb_pair);
        self.liquidity_manager.remove_liquidity(state).await?;

        // 购买基础代币
        let amount_y_for_buy = position
            .amount_y
            .safe_div(2)
            .map_err(|_| Error::msg("数值溢出"))?;
            
        let (amount_x, amount_y) = if amount_y_for_buy != 0 {
            info!("交换代币 {}", state.lb_pair);
            let swap_event = self.swap_manager.swap(state, amount_y_for_buy, false).await?;
            (swap_event.amount_out, position.amount_y - amount_y_for_buy)
        } else {
            (pair_config.x_amount, pair_config.y_amount)
        };

        // 重新添加流动性
        info!("添加流动性 {}", state.lb_pair);
        self.liquidity_manager.add_liquidity(state, amount_x, amount_y, state.lb_pair_state.active_id).await?;
        
        // 刷新状态
        info!("刷新状态 {}", state.lb_pair);
        self.liquidity_manager.refresh_state().await?;
        
        Ok(())
    }
    
    // 向左调整仓位 (价格下跌时)
    async fn shift_left(&self, state: &SinglePosition) -> Result<()> {
        let pair_config = get_pair_config(&self.config, state.lb_pair);
        info!("向左调整仓位 {}", state.lb_pair);
        
        // 获取当前仓位状态
        let position = state.get_positions()?;
        if position.amount_y != 0 {
            return Err(Error::msg("Y代币数量不为零，不能向左调整"));
        }
        
        // 取出流动性
        info!("提取流动性 {}", state.lb_pair);
        self.liquidity_manager.remove_liquidity(state).await?;

        // 出售基础代币
        let amount_x_for_sell = position
            .amount_x
            .safe_div(2)
            .map_err(|_| Error::msg("数值溢出"))?;
            
        let (amount_x, amount_y) = if amount_x_for_sell != 0 {
            info!("交换代币 {}", state.lb_pair);
            let swap_event = self.swap_manager.swap(state, amount_x_for_sell, true).await?;
            (position.amount_x - amount_x_for_sell, swap_event.amount_out)
        } else {
            (pair_config.x_amount, pair_config.y_amount)
        };

        // 重新添加流动性
        info!("添加流动性 {}", state.lb_pair);
        self.liquidity_manager.add_liquidity(state, amount_x, amount_y, state.lb_pair_state.active_id).await?;
        
        // 刷新状态
        info!("刷新状态 {}", state.lb_pair);
        self.liquidity_manager.refresh_state().await?;
        
        Ok(())
    }
}