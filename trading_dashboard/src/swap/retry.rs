use crate::state::SinglePosition;
use crate::wallet::WalletManager;
use anyhow::Result;
use lb_clmm::events::Swap as SwapEvent;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use log::warn;

use super::Jupiter;

pub struct RetrySwap {
    pub wallet_manager: Arc<WalletManager>,
}

impl RetrySwap {
    pub fn new(wallet_manager: Arc<WalletManager>) -> Self {
        Self { wallet_manager }
    }
    
    pub async fn swap_with_retry(
        &self,
        state: &SinglePosition,
        amount_in: u64,
        swap_for_y: bool,
        max_retries: u8,
    ) -> Result<SwapEvent> {
        let jupiter = Jupiter::new(self.wallet_manager.clone());
        
        // 重试逻辑
        let mut retry_count = 0;
        loop {
            match jupiter.swap(state, amount_in, swap_for_y, false).await {
                Ok(event) => return Ok(event),
                Err(e) => {
                    retry_count += 1;
                    if retry_count >= max_retries {
                        return Err(e);
                    }
                    
                    warn!("交换失败，尝试重试 {}/{}...: {:?}", retry_count, max_retries, e);
                    sleep(Duration::from_secs(2)).await;
                }
            }
        }
    }
}
