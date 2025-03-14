use anchor_client::solana_sdk::signature::Keypair;
use anyhow::*;
use solana_sdk::signature::read_keypair_file;
use std::path::Path;
use bs58;
use hmac::Hmac;
use sha2::Sha512;
use pbkdf2::pbkdf2;

// 从助记词创建钱包（简化版，仅用于测试）
// 警告：这不是生产环境下推荐的实现方式
pub fn keypair_from_seed(seed_phrase: &str) -> Result<Keypair> {
    // 使用简单哈希生成种子
    let mut seed = [0u8; 32];
    let salt = b"solana-keygen";
    
    // pbkdf2返回()，直接调用不需要检查错误
    pbkdf2::<Hmac<Sha512>>(
        seed_phrase.as_bytes(),
        salt,
        2048,
        &mut seed,
    );
    
    // 从种子创建密钥对
    Keypair::from_bytes(&seed)
        .map_err(|_| anyhow!("从种子创建密钥对失败"))
}

// 从Base58字符串创建钱包
pub fn keypair_from_base58(base58_str: &str) -> Result<Keypair> {
    let bytes = bs58::decode(base58_str)
        .into_vec()
        .map_err(|_| anyhow!("无效的Base58字符串"))?;
        
    Keypair::from_bytes(&bytes)
        .map_err(|_| anyhow!("无效的密钥对字节"))
}

// 读取钱包
pub fn read_wallet(path_or_key: &Option<String>, secret_key: &Option<String>) -> Result<Keypair> {
    // 优先使用密钥字符串
    if let Some(key) = secret_key {
        // 尝试作为Base58字符串解析
        let base58_result = keypair_from_base58(key);
        if base58_result.is_ok() {
            return base58_result;
        }
        
        // 尝试作为种子短语使用
        return keypair_from_seed(key);
    }
    
    // 如果没有密钥，尝试从文件读取
    if let Some(path) = path_or_key {
        if Path::new(path).exists() {
            return read_keypair_file(path)
                .map_err(|_| anyhow!(format!("无法读取钱包文件: {}", path)));
        }
    }
    
    Err(anyhow!("未提供钱包密钥或有效的钱包文件路径"))
}