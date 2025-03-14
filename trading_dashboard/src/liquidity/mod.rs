mod pool;
mod position;
mod deposit;
mod withdraw;

pub use pool::*;
pub use position::*;
pub use deposit::*;
pub use withdraw::*;

use crate::state::{AllPosition, SinglePosition};
use crate::wallet::WalletManager;
use anyhow::*;

pub struct LiquidityManager {
    pub wallet_manager: WalletManager,
    pub state: Arc<Mutex<AllPosition>>,
}

impl LiquidityManager {
    pub fn new(wallet_manager: WalletManager, state: Arc<Mutex<AllPosition>>) -> Self {
        Self { wallet_manager, state }
    }

    // 添加流动性(从Core.deposit迁移)
    pub async fn add_liquidity(&self, state: &SinglePosition, amount_x: u64, amount_y: u64, active_id: i32) -> Result<()> {
        // 调用src/liquidity/deposit.rs中的实现
        let deposit = Deposit::new(self.wallet_manager.clone(), self.state.clone());
        deposit.deposit(state, amount_x, amount_y, active_id, false).await
    }

    // 移除流动性(从Core.withdraw迁移)
    pub async fn remove_liquidity(&self, state: &SinglePosition) -> Result<()> {
        // 调用src/liquidity/withdraw.rs中的实现
        let withdraw = Withdraw::new(self.wallet_manager.clone(), self.state.clone());
        withdraw.withdraw(state, false).await
    }
}