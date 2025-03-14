use anyhow::Result;
use anchor_client::solana_sdk::pubkey::Pubkey;
use solana_client::rpc_client::RpcClient;

pub async fn get_sol_balance(client: &RpcClient, pubkey: &Pubkey) -> Result<f64> {
    let lamports = client.get_balance(pubkey)?;
    Ok(lamports as f64 / 1_000_000_000.0) // 转换为SOL
}

pub async fn get_token_balance(
    client: &RpcClient,
    token_account: &Pubkey
) -> Result<u64> {
    let account = client.get_token_account(token_account)?
        .ok_or_else(|| anyhow::anyhow!("找不到代币账户"))?;
    
    // 解析 String 类型的数量为 u64
    let amount_str = account.token_amount.amount.clone();
    let amount = amount_str.parse::<u64>()
        .map_err(|_| anyhow::anyhow!("无法解析代币数量"))?;
    
    Ok(amount)
} 