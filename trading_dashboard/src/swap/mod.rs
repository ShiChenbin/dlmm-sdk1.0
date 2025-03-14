mod jupiter;
mod retry;

pub use jupiter::*;
pub use retry::*;

use crate::state::SinglePosition;
use crate::wallet::WalletManager;
use anyhow::Result;
use lb_clmm::events::Swap as SwapEvent;
use std::sync::Arc;

pub struct SwapManager {
    pub wallet_manager: Arc<WalletManager>,
}

impl SwapManager {
    pub fn new(wallet_manager: Arc<WalletManager>) -> Self {
        Self { wallet_manager }
    }

    // 代币交换
    pub async fn swap(&self, state: &SinglePosition, amount_in: u64, swap_for_y: bool) -> Result<SwapEvent> {
        // 调用src/swap/jupiter.rs中的实现
        let jupiter = Jupiter::new(self.wallet_manager.clone());
        jupiter.swap(state, amount_in, swap_for_y, false).await
    }
    
    // 带重试的交换
    pub async fn swap_with_retry(&self, state: &SinglePosition, amount_in: u64, swap_for_y: bool, max_retries: u8) -> Result<SwapEvent> {
        // 调用src/swap/retry.rs中的实现
        let retry = RetrySwap::new(self.wallet_manager.clone());
        retry.swap_with_retry(state, amount_in, swap_for_y, max_retries).await
    }
}