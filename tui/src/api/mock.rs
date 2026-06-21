use super::types::*;
use super::WalletApi;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;

pub struct MockWalletApi {
    wallet_exists: bool,
    home_cache: Mutex<Option<HomeData>>,
    send_cache: Mutex<HashMap<String, SendData>>,
    receive_cache: Mutex<HashMap<String, ReceiveData>>,
}

impl MockWalletApi {
    pub fn new() -> Self {
        Self {
            wallet_exists: false,
            home_cache: Mutex::new(None),
            send_cache: Mutex::new(HashMap::new()),
            receive_cache: Mutex::new(HashMap::new()),
        }
    }

    pub fn set_wallet_exists(&mut self, exists: bool) {
        self.wallet_exists = exists;
    }
}

#[async_trait(?Send)]
impl WalletApi for MockWalletApi {
    async fn get_setup(&self) -> SetupData {
        SetupData {
            app_version: "1.0.0".to_string(),
            wallet_exists: self.wallet_exists,
            new_mnemonic: vec![
                "ribbon".into(),
                "velvet".into(),
                "ocean".into(),
                "puzzle".into(),
                "harvest".into(),
                "guitar".into(),
                "shadow".into(),
                "ladder".into(),
                "comfort".into(),
                "raven".into(),
                "spring".into(),
                "anchor".into(),
            ],
            word_count: 12,
            import_methods: vec!["mnemonic".into(), "privateKey".into()],
        }
    }

    async fn submit_setup_create(&self, _input: SetupCreateInput) -> Result<(), ApiError> {
        Ok(())
    }

    async fn submit_setup_import(&self, _input: SetupImportInput) -> Result<(), ApiError> {
        Ok(())
    }

    async fn get_wallets(&self) -> WalletsData {
        WalletsData {
            wallets: vec![
                WalletDerivation {
                    index: 0,
                    address: "0x742d35Cc6634C0532925a3b844Bc454e4438f44e".into(),
                    chain_id: "eip155:1".into(),
                    chain_name: "Ethereum".into(),
                },
                WalletDerivation {
                    index: 1,
                    address: "0x8f3E8A8e8b8C8d8E8f8A8b8C8d8E8f8A8b8C8d8E".into(),
                    chain_id: "eip155:1".into(),
                    chain_name: "Ethereum".into(),
                },
                WalletDerivation {
                    index: 2,
                    address: "0x1a2B3c4D5e6F7a8B9c0D1e2F3a4B5c6D7e8F9a0B".into(),
                    chain_id: "eip155:1".into(),
                    chain_name: "Ethereum".into(),
                },
                WalletDerivation {
                    index: 3,
                    address: "t1YhnKpPk6KxqGHgK7LKzK5qLpK5qLpK5qL".into(),
                    chain_id: "bip122:00040fe8ec8471911baa1f7c215a71e9".into(),
                    chain_name: "Zcash".into(),
                },
            ],
        }
    }

    async fn get_assets(&self, chain_id: &str) -> AssetsData {
        if chain_id.contains("bip122") {
            AssetsData {
                assets: vec![AssetRow {
                    name: "Zcash".into(),
                    ticker: "ZEC".into(),
                    price: "$28.50".into(),
                    price_change: "▲ 1.25%".into(),
                    price_change_up: true,
                    holdings_value: "$142.50".into(),
                    holdings_amount: "5 ZEC".into(),
                    chain_id: chain_id.into(),
                }],
            }
        } else {
            AssetsData {
                assets: vec![
                    AssetRow {
                        name: "Ethereum".into(),
                        ticker: "ETH".into(),
                        price: "$2,000.00".into(),
                        price_change: "▲ 5.45%".into(),
                        price_change_up: true,
                        holdings_value: "$4,000.00".into(),
                        holdings_amount: "2 ETH".into(),
                        chain_id: chain_id.into(),
                    },
                    AssetRow {
                        name: "Wrapped Bitcoin".into(),
                        ticker: "WBTC".into(),
                        price: "$60,000.00".into(),
                        price_change: "▼ 0.15%".into(),
                        price_change_up: false,
                        holdings_value: "$1,000.00".into(),
                        holdings_amount: "0.0001 WBTC".into(),
                        chain_id: chain_id.into(),
                    },
                    AssetRow {
                        name: "USD Coin".into(),
                        ticker: "USDC".into(),
                        price: "$1.00".into(),
                        price_change: "▲ 0.01%".into(),
                        price_change_up: true,
                        holdings_value: "$500.00".into(),
                        holdings_amount: "500 USDC".into(),
                        chain_id: chain_id.into(),
                    },
                    AssetRow {
                        name: "Chainlink".into(),
                        ticker: "LINK".into(),
                        price: "$14.25".into(),
                        price_change: "▼ 2.10%".into(),
                        price_change_up: false,
                        holdings_value: "$285.00".into(),
                        holdings_amount: "20 LINK".into(),
                        chain_id: chain_id.into(),
                    },
                    AssetRow {
                        name: "Uniswap".into(),
                        ticker: "UNI".into(),
                        price: "$7.80".into(),
                        price_change: "▲ 1.20%".into(),
                        price_change_up: true,
                        holdings_value: "$156.00".into(),
                        holdings_amount: "20 UNI".into(),
                        chain_id: chain_id.into(),
                    },
                ],
            }
        }
    }

    async fn get_home(&self) -> HomeData {
        HomeData {
            accounts: vec![
                AccountInfo {
                    account_id: "acc_1".into(),
                    chain_id: "eip155:1".into(),
                    address: "0x742d35Cc6634C0532925a3b844Bc454e4438f44e".into(),
                },
                AccountInfo {
                    account_id: "acc_2".into(),
                    chain_id: "bip122:00040fe8ec8471911baa1f7c215a71e9".into(),
                    address: "t1YhnKpPk6KxqGHgK7LKzK5qLpK5qLpK5qL".into(),
                },
            ],
            balances: vec![
                BalanceInfo {
                    token_id: "eth-native".into(),
                    chain_id: "eip155:1".into(),
                    symbol: "ETH".into(),
                    decimals: 18,
                    raw_balance: "1420000000000000000".into(),
                    fiat_value: 4956.20,
                },
                BalanceInfo {
                    token_id: "zec-native".into(),
                    chain_id: "bip122:00040fe8ec8471911baa1f7c215a71e9".into(),
                    symbol: "ZEC".into(),
                    decimals: 8,
                    raw_balance: "500000000".into(),
                    fiat_value: 142.50,
                },
            ],
            total_fiat_value: 5098.70,
            fiat_currency: "USD".into(),
            pending_tx: Some(PendingTx {
                tx_hash: "0x4a7db3c8d2e1f0a4b6c8d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2".into(),
                status: "pending".into(),
                block_explorer_url: "https://etherscan.io/tx/0x4a7db3c8d2e1f0a4b6c8d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2".into(),
            }),
        }
    }

    async fn submit_home(&self, _input: HomeInput) -> HomeData {
        self.get_home().await
    }

    async fn get_receive(&self, _chain_id: &str) -> ReceiveData {
        ReceiveData {
            address: "0x742d35Cc6634C0532925a3b844Bc454e4438f44e".into(),
            chain_id: "eip155:1".into(),
            address_format: "hex".into(),
            qr_payload: "ethereum:0x742d35Cc6634C0532925a3b844Bc454e4438f44e".into(),
        }
    }

    async fn submit_receive(&self, input: ReceiveInput) -> ReceiveData {
        if input.selected_chain_id.contains("bip122") {
            ReceiveData {
                address: "t1YhnKpPk6KxqGHgK7LKzK5qLpK5qLpK5qL".into(),
                chain_id: "bip122:00040fe8ec8471911baa1f7c215a71e9".into(),
                address_format: "transparent".into(),
                qr_payload: "zcash:t1YhnKpPk6KxqGHgK7LKzK5qLpK5qLpK5qL".into(),
            }
        } else {
            self.get_receive("").await
        }
    }

    async fn get_send(&self, chain_id: &str) -> SendData {
        if chain_id.contains("bip122") {
            SendData {
                from_address: "t1YhnKpPk6KxqGHgK7LKzK5qLpK5qLpK5qL".into(),
                spendable_balance: "500000000".into(),
                decimals: 8,
                chain_id: "bip122:00040fe8ec8471911baa1f7c215a71e9".into(),
                fee_data: FeeData::Zec(FeeRates {
                    slow: 8,
                    medium: 21,
                    fast: 45,
                }),
                nonce: None,
                utxos: Some(vec![
                    UtxoInfo {
                        txid: "3f8c...d29a".into(),
                        vout: 0,
                        value: "300000000".into(),
                    },
                    UtxoInfo {
                        txid: "7b1e...44f0".into(),
                        vout: 1,
                        value: "200000000".into(),
                    },
                ]),
            }
        } else {
            SendData {
                from_address: "0x742d35Cc6634C0532925a3b844Bc454e4438f44e".into(),
                spendable_balance: "1420000000000000000".into(),
                decimals: 18,
                chain_id: "eip155:1".into(),
                fee_data: FeeData::Eth(FeeDataEth {
                    base_fee_per_gas: "18000000000".into(),
                    max_priority_fee_per_gas: "1500000000".into(),
                    gas_limit_estimate: "21000".into(),
                }),
                nonce: Some(42),
                utxos: None,
            }
        }
    }

    async fn submit_send_review(&self, input: SendReviewInput) -> SendReviewData {
        let fee_est = match &input.fee_selection.tier[..] {
            "slow" => "300000000000000",
            "fast" => "500000000000000",
            _ => "409500000000000",
        };
        let total = format!(
            "{}",
            input.amount.parse::<u128>().unwrap_or(0) + fee_est.parse::<u128>().unwrap_or(0)
        );
        SendReviewData {
            to_address: input.to_address,
            amount: input.amount,
            fee_estimate: fee_est.to_string(),
            total_amount: total,
            chain_id: input.chain_id,
        }
    }

    async fn submit_send_confirm(&self, _input: SendConfirmInput) -> SendResult {
        let tx_hash: String =
            "0x02f8b00182002a8459682f00851b572f4e9a7b3c8d2e1f0a4b6c8d0e1f2a3b4c5d6e7f8a9b".into();
        SendResult {
            tx_hash: tx_hash.clone(),
            status: "broadcasted".into(),
            block_explorer_url: format!("https://etherscan.io/tx/{}", tx_hash),
        }
    }

    async fn get_lock(&self) -> LockData {
        LockData {
            auth_methods: LockAuthMethods {
                biometric_available: true,
                password_set: true,
            },
            failed_attempts: 0,
        }
    }

    async fn submit_lock(&self, _input: LockInput) -> Result<(), ApiError> {
        Ok(())
    }

    async fn get_settings(&self) -> SettingsData {
        SettingsData {
            security: SecuritySettings {
                biometric_enabled: true,
                auto_lock_minutes: 5,
            },
            fiat_currency: "USD".into(),
            app_version: "1.0.0".into(),
        }
    }

    async fn submit_settings(&self, _input: SettingsInput) -> Result<(), ApiError> {
        Ok(())
    }

    async fn submit_reveal_phrase(
        &self,
        _input: RevealPhraseInput,
    ) -> Result<Vec<String>, ApiError> {
        Ok(vec![
            "ribbon".into(),
            "velvet".into(),
            "ocean".into(),
            "puzzle".into(),
            "harvest".into(),
            "guitar".into(),
            "shadow".into(),
            "ladder".into(),
            "comfort".into(),
            "raven".into(),
            "spring".into(),
            "anchor".into(),
        ])
    }

    async fn check_wallet_exists(&self) -> bool {
        false
    }

    async fn unlock(&self, _password: String) -> Result<UnlockData, ApiError> {
        Ok(UnlockData { accounts_count: 2 })
    }

    async fn home_state(&self) -> ApiState<HomeData> {
        let should_fetch = self.home_cache.lock().unwrap().is_none();
        if should_fetch {
            let data = self.get_home().await;
            *self.home_cache.lock().unwrap() = Some(data);
        }
        ApiState::Loaded(self.home_cache.lock().unwrap().as_ref().unwrap().clone())
    }

    async fn refresh_home(&self) {
        *self.home_cache.lock().unwrap() = None;
    }

    async fn receive_state(&self, chain_id: &str) -> ApiState<ReceiveData> {
        let data = {
            let cache = self.receive_cache.lock().unwrap();
            cache.get(chain_id).cloned()
        };
        if let Some(data) = data {
            return ApiState::Loaded(data);
        }
        let real = self.get_receive(chain_id).await;
        self.receive_cache
            .lock()
            .unwrap()
            .insert(chain_id.to_string(), real.clone());
        ApiState::Loaded(real)
    }

    async fn refresh_receive(&self, chain_id: &str) {
        self.receive_cache.lock().unwrap().remove(chain_id);
    }

    async fn send_state(&self, chain_id: &str) -> ApiState<SendData> {
        let data = {
            let cache = self.send_cache.lock().unwrap();
            cache.get(chain_id).cloned()
        };
        if let Some(data) = data {
            return ApiState::Loaded(data);
        }
        let real = self.get_send(chain_id).await;
        self.send_cache
            .lock()
            .unwrap()
            .insert(chain_id.to_string(), real.clone());
        ApiState::Loaded(real)
    }

    async fn refresh_send(&self, chain_id: &str) {
        self.send_cache.lock().unwrap().remove(chain_id);
    }
}
