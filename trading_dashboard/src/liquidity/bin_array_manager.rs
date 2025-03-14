use anyhow::*;
use lb_clmm::state::bin::{Bin, BinArray};
use lb_clmm::state::position::PositionV2;

// 添加这个辅助函数，支持u128类型参数
fn mul_div_floor(a: u64, b: u128, c: u128) -> Option<u64> {
    if c == 0 {
        return None;
    }
    let product = (a as u128).checked_mul(b)?;
    Some((product / c) as u64)
}

// 计算bin数组的起始bin ID
fn get_start_bin_id(bin_array_index: i64) -> i32 {
    // 根据lb_clmm库，每个bin数组包含70个bin
    // bin_id = bin_array_index * 70
    (bin_array_index * 70) as i32
}

pub struct BinArrayManager<'a> {
    pub bin_arrays: &'a [BinArray],
}

impl<'a> BinArrayManager<'a> {
    // 从bin ID获取bin
    pub fn get_bin(&self, bin_id: i32) -> Result<&Bin> {
        for bin_array in self.bin_arrays.iter() {
            let index = (bin_id - get_start_bin_id(bin_array.index as i64)) as usize;
            if index < bin_array.bins.len() {
                return Ok(bin_array.get_bin(bin_id).map_err(|e| anyhow!("获取bin失败: {:?}", e))?);
            }
        }
        Err(anyhow!(format!("Cannot find bin id {}", bin_id)))
    }
    
    // 获取待收取的总手续费
    pub fn get_total_fee_pending(&self, position: &PositionV2) -> Result<(u64, u64)> {
        let mut fee_x = 0u64;
        let mut fee_y = 0u64;
        
        for (i, _) in position.liquidity_shares.iter().enumerate() {
            if position.liquidity_shares[i] == 0 {
                continue;
            }
            
            let bin_id = position.from_idx_to_bin_id(i)?;
            for bin_array in self.bin_arrays.iter() {
                let index = (bin_id - get_start_bin_id(bin_array.index as i64)) as usize;
                if index < bin_array.bins.len() {
                    let bin = bin_array.get_bin(bin_id).map_err(|e| anyhow!("获取bin失败: {:?}", e))?;
                    
                    // 计算fee amount
                    let liquidity_share = position.liquidity_shares[i];
                    let (fee_x_pending, fee_y_pending) = (
                        mul_div_floor(bin.amount_x, liquidity_share, bin.liquidity_supply).unwrap_or(0),
                        mul_div_floor(bin.amount_y, liquidity_share, bin.liquidity_supply).unwrap_or(0)
                    );
                    
                    fee_x = fee_x.checked_add(fee_x_pending).ok_or_else(|| anyhow!("Math overflow"))?;
                    fee_y = fee_y.checked_add(fee_y_pending).ok_or_else(|| anyhow!("Math overflow"))?;
                    break;
                }
            }
        }
        
        Ok((fee_x, fee_y))
    }
} 