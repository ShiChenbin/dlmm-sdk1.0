#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_keypair_from_seed() {
        // 使用简单的测试种子
        let seed = "test wallet seed phrase for development only";
        let result = keypair_from_seed(seed);
        assert!(result.is_ok());
    }
    
    #[test]
    fn test_wallet_manager() {
        let config = Arc::new(Mutex::new(Config::default()));
        let wallet_manager = WalletManager::new(config.clone());
        
        // 设置测试密钥
        config.lock().unwrap().wallet_mnemonic = Some("test wallet seed phrase for development only".to_string());
        
        // 测试初始化
        let init_result = wallet_manager.initialize_wallet();
        assert!(init_result.is_ok());
        
        // 测试获取地址
        let address_result = wallet_manager.get_wallet_address();
        assert!(address_result.is_ok());
    }
} 