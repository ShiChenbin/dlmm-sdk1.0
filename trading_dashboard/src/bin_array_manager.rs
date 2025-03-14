use anyhow::*;
use lb_clmm::state::bin::{Bin, BinArray};
use lb_clmm::state::position::PositionV2;

pub struct BinArrayManager<'a> {
    pub bin_arrays: &'a [BinArray],
}

impl<'a> BinArrayManager<'a> {
    // 从bin ID获取bin
    pub fn get_bin(&self, bin_id: i32) -> Result<&Bin> {
        for bin_array in self.bin_arrays.iter() {
            if bin_array.contains(bin_id) {
                return bin_array.get_bin(bin_id);
            }
        }
        Err(Error::msg(format!("Cannot find bin id {}", bin_id)))
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
                if bin_array.contains(bin_id) {
                    let bin = bin_array.get_bin(bin_id)?;
                    let (fee_x_pending, fee_y_pending) = bin.get_fee_amount(position.liquidity_shares[i])?;
                    fee_x = fee_x.checked_add(fee_x_pending).ok_or(Error::msg("Math overflow"))?;
                    fee_y = fee_y.checked_add(fee_y_pending).ok_or(Error::msg("Math overflow"))?;
                    break;
                }
            }
        }
        
        Ok((fee_x, fee_y))
    }
} 