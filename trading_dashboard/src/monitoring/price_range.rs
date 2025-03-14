use crate::state::SinglePosition;
use anyhow::*;

pub struct PriceRangeMonitor;

impl PriceRangeMonitor {
    // 检查价格是否在区间内
    pub fn is_price_in_range(position: &SinglePosition) -> bool {
        let active_id = position.lb_pair_state.active_id;
        active_id >= position.min_bin_id && active_id <= position.max_bin_id
    }
    
    // 检查价格是否低于下限
    pub fn is_price_below_range(position: &SinglePosition) -> bool {
        position.lb_pair_state.active_id < position.min_bin_id
    }
    
    // 检查价格是否高于上限
    pub fn is_price_above_range(position: &SinglePosition) -> bool {
        position.lb_pair_state.active_id > position.max_bin_id
    }
    
    // 计算当前价格与上下限的距离
    pub fn calculate_price_distance(position: &SinglePosition) -> (i32, i32) {
        let active_id = position.lb_pair_state.active_id;
        let distance_to_min = active_id - position.min_bin_id;
        let distance_to_max = position.max_bin_id - active_id;
        (distance_to_min, distance_to_max)
    }
} 