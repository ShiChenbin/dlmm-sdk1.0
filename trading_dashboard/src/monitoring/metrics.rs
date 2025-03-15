use crate::state::SinglePosition;
use crate::liquidity::PositionInfo;
use anyhow::{Result, anyhow};
use reqwest::Client;
use serde_json;
use log;

const METEORA_API_URL: &str = "https://api.meteora.ag/api"; // Meteora API的基础URL

pub struct MetricsCollector;

impl MetricsCollector {
    pub fn new() -> Self {
        Self {}
    }
    
    pub fn calculate_apy(
        &self, 
        position: &SinglePosition, 
        days: u64
    ) -> Result<f64> {
        // 简单的APY计算示例
        // 实际实现需要使用手续费收入和天数
        Ok(0.05) // 示例：5%
    }
    
    pub fn get_price_volatility(
        &self,
        position: &SinglePosition
    ) -> Result<f64> {
        // 计算价格波动率
        // 实际实现需要历史价格数据
        Ok(0.02) // 示例：2%
    }

    /// 查询池子的24小时费用/TVL比率
    pub async fn fetch_fee_tvl_ratio(&self, pair_address: &str, num_of_days: u64) -> Result<f64> {
        let client = Client::new();
        
        // 查询费用
        let fee_url = format!("{}/stat/pair-volume/{}/{}", METEORA_API_URL, pair_address, num_of_days);
        let fee_response = client.get(&fee_url)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;
        
        let fee_amount = fee_response["totalFeeUSD"]
            .as_f64()
            .ok_or_else(|| anyhow!("无法获取费用信息"))?;
        
        // 查询TVL
        let tvl_url = format!("{}/pool/{}", METEORA_API_URL, pair_address);
        let tvl_response = client.get(&tvl_url)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;
        
        let tvl = tvl_response["tvl"]
            .as_f64()
            .ok_or_else(|| anyhow!("无法获取TVL信息"))?;
        
        // 计算比率
        if tvl == 0.0 {
            return Err(anyhow!("TVL为零，无法计算比率"));
        }
        
        Ok(fee_amount / tvl)
    }
    
    /// 获取多个时间段的平均费用/TVL比率
    pub async fn get_avg_fee_tvl_ratio(&self, pair_address: &str, days_periods: &[u64]) -> Result<f64> {
        let mut total_ratio = 0.0;
        let mut valid_periods = 0;
        
        for days in days_periods {
            match self.fetch_fee_tvl_ratio(pair_address, *days).await {
                Ok(ratio) => {
                    total_ratio += ratio;
                    valid_periods += 1;
                },
                Err(e) => {
                    log::warn!("获取{}天期间的费用/TVL比率失败: {}", days, e);
                    // 继续处理其他时间段
                }
            }
        }
        
        if valid_periods == 0 {
            return Err(anyhow!("所有时间段的费用/TVL比率获取均失败"));
        }
        
        Ok(total_ratio / valid_periods as f64)
    }
} 