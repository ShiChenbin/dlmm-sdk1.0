mod pool;
mod position;
mod deposit;
mod withdraw;
mod bin_array_manager;

pub use pool::*;
pub use position::*;
pub use deposit::*;
pub use withdraw::*;
pub use bin_array_manager::*;

use crate::state::{AllPosition, SinglePosition};
use crate::wallet::WalletManager;
use anchor_client::solana_sdk::pubkey::Pubkey;
use std::sync::{Arc, Mutex};
use anyhow::*;

pub struct LiquidityManager {
    pub wallet_manager: Arc<WalletManager>,
    pub state: Arc<Mutex<AllPosition>>,
    pub position_manager: Arc<PositionManager>,
    pub deposit_manager: Arc<Deposit>,
    pub withdraw_manager: Arc<Withdraw>,
}

impl LiquidityManager {
    pub fn new(wallet_manager: Arc<WalletManager>, state: Arc<Mutex<AllPosition>>) -> Self {
        let position_manager = Arc::new(PositionManager::new(state.clone()));
        let deposit_manager = Arc::new(Deposit::new(wallet_manager.clone()));
        let withdraw_manager = Arc::new(Withdraw::new(wallet_manager.clone()));
        
        Self { 
            wallet_manager,
            state, 
            position_manager,
            deposit_manager,
            withdraw_manager
        }
    }

    // 添加流动性
    pub async fn add_liquidity(&self, state: &SinglePosition, amount_x: u64, amount_y: u64, active_id: i32) -> Result<()> {
        self.deposit_manager.deposit(state, amount_x, amount_y, active_id, false).await
    }

    // 移除流动性
    pub async fn remove_liquidity(&self, state: &SinglePosition) -> Result<()> {
        self.withdraw_manager.withdraw(state, false).await
    }
    
    // 刷新状态
    pub async fn refresh_state(&self) -> Result<()> {
        // 这里需要实现状态刷新逻辑
        // TODO: 实现从链上获取最新状态
        Ok(())
    }
    
    // 获取特定池子的仓位
    pub fn get_position(&self, lb_pair: Pubkey) -> Option<SinglePosition> {
        self.position_manager.get_position(lb_pair)
    }
    
    // 获取所有仓位信息
    pub fn get_all_positions_info(&self) -> Result<Vec<PositionInfo>> {
        self.position_manager.get_positions_info()
    }
}