use anchor_client::solana_sdk::signature::Keypair;
use anchor_client::solana_sdk::signer::Signer;
use anyhow::*;
use solana_sdk::derivation_path::DerivationPath;
use solana_sdk::signature::read_keypair_file;
use std::path::Path;
use tiny_bip39::{Mnemonic, Language, Seed};
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

// 从助记词创建钱包
pub fn keypair_from_mnemonic(mnemonic: &str, passphrase: &str) -> Result<Keypair> {
    let mnemonic = Mnemonic::from_phrase(mnemonic, Language::English)
        .map_err(|e| Error::msg(format!("无效的助记词: {}", e)))?;
    
    let seed = Seed::new(&mnemonic, passphrase);
    let seed_bytes = seed.as_bytes();
    
    // 使用默认的衍生路径 m/44'/501'/0'/0'
    let path = DerivationPath::from_str("m/44'/501'/0'/0'")
        .map_err(|e| Error::msg(format!("无效的衍生路径: {}", e)))?;
    
    let keypair = keypair_from_seed_and_path(seed_bytes, &path)
        .map_err(|e| Error::msg(format!("从种子创建密钥对失败: {}", e)))?;
    
    Ok(keypair)
}

// 从文件或助记词读取钱包
pub fn read_wallet(path_or_mnemonic: &Option<String>, mnemonic_str: &Option<String>) -> Result<Keypair> {
    // 优先使用助记词
    if let Some(mnemonic) = mnemonic_str {
        return keypair_from_mnemonic(mnemonic, ""); // 空密码
    }
    
    // 如果没有助记词，尝试从文件读取
    if let Some(path) = path_or_mnemonic {
        if Path::new(path).exists() {
            return read_keypair_file(path)
                .map_err(|_| Error::msg(format!("无法读取钱包文件: {}", path)));
        }
    }
    
    // 都没有则返回错误
    Err(Error::msg("未提供钱包助记词或有效的钱包文件路径"))
}

// 辅助函数 - 从种子和路径创建密钥对
fn keypair_from_seed_and_path(seed: &[u8], path: &DerivationPath) -> std::result::Result<Keypair, Box<dyn std::error::Error>> {
    let ed25519_derivation = ed25519_dalek::SigningKey::derive_from_path(seed, path)?;
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&ed25519_derivation.as_ref());
    Ok(Keypair::from_bytes(&signing_key.to_bytes())?)
}