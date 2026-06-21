use super::types::*;
use super::WalletApi;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;

struct MockData {
    accounts: Vec<AccountInfo>,
    next_account_index: u32,
}

pub struct MockWalletApi {
    wallet_exists: bool,
    data: Mutex<MockData>,
    home_cache: Mutex<Option<HomeData>>,
    send_cache: Mutex<HashMap<String, SendData>>,
    receive_cache: Mutex<HashMap<String, ReceiveData>>,
}

impl MockWalletApi {
    pub fn new() -> Self {
        Self {
            wallet_exists: false,
            data: Mutex::new(MockData {
                accounts: vec![
                    AccountInfo {
                        account_id: "acc_1".into(),
                        name: "Ethereum Wallet".into(),
                        chain_id: "eip155:1".into(),
                        address: "0x742d35Cc6634C0532925a3b844Bc454e4438f44e".into(),
                        protocol: "Ethereum".into(),
                    },
                    AccountInfo {
                        account_id: "acc_2".into(),
                        name: "Zcash Wallet".into(),
                        chain_id: "bip122:00040fe8ec8471911baa1f7c215a71e9".into(),
                        address: "t1YhnKpPk6KxqGHgK7LKzK5qLpK5qLpK5qL".into(),
                        protocol: "Zcash".into(),
                    },
                ],
                next_account_index: 3,
            }),
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

    async fn get_assets(&self, account_id: &str) -> AssetsData {
        if account_id.contains("bip122") {
            AssetsData {
                assets: vec![AssetRow {
                    name: "Zcash".into(),
                    ticker: "ZEC".into(),
                    price: "$28.50".into(),
                    price_change: "▲ 1.25%".into(),
                    price_change_up: true,
                    holdings_value: "$142.50".into(),
                    holdings_amount: "5 ZEC".into(),
                    chain_id: account_id.into(),
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
                        chain_id: account_id.into(),
                    },
                    AssetRow {
                        name: "Wrapped Bitcoin".into(),
                        ticker: "WBTC".into(),
                        price: "$60,000.00".into(),
                        price_change: "▼ 0.15%".into(),
                        price_change_up: false,
                        holdings_value: "$1,000.00".into(),
                        holdings_amount: "0.0001 WBTC".into(),
                        chain_id: account_id.into(),
                    },
                    AssetRow {
                        name: "USD Coin".into(),
                        ticker: "USDC".into(),
                        price: "$1.00".into(),
                        price_change: "▲ 0.01%".into(),
                        price_change_up: true,
                        holdings_value: "$500.00".into(),
                        holdings_amount: "500 USDC".into(),
                        chain_id: account_id.into(),
                    },
                    AssetRow {
                        name: "Chainlink".into(),
                        ticker: "LINK".into(),
                        price: "$14.25".into(),
                        price_change: "▼ 2.10%".into(),
                        price_change_up: false,
                        holdings_value: "$285.00".into(),
                        holdings_amount: "20 LINK".into(),
                        chain_id: account_id.into(),
                    },
                    AssetRow {
                        name: "Uniswap".into(),
                        ticker: "UNI".into(),
                        price: "$7.80".into(),
                        price_change: "▲ 1.20%".into(),
                        price_change_up: true,
                        holdings_value: "$156.00".into(),
                        holdings_amount: "20 UNI".into(),
                        chain_id: account_id.into(),
                    },
                ],
            }
        }
    }

    async fn get_home(&self) -> HomeData {
        let data = self.data.lock().unwrap();
        HomeData {
            accounts: data.accounts.clone(),
            fiat_currency: "USD".into(),
        }
    }

    async fn submit_home(&self, _input: HomeInput) -> HomeData {
        self.get_home().await
    }

    async fn list_accounts(&self) -> Result<Vec<AccountInfo>, ApiError> {
        let data = self.data.lock().unwrap();
        Ok(data.accounts.clone())
    }

    async fn add_account(&self) -> Result<(), ApiError> {
        let mut data = self.data.lock().unwrap();
        let index = data.next_account_index;
        data.next_account_index += 1;

        let account_id = format!("acc_{}", index);
        let is_eth = index % 2 == 0;
        let (name, chain_id, address, protocol) = if is_eth {
            (
                format!("Ethereum Account {}", index),
                "eip155:1".into(),
                format!("0x{:040x}", index),
                "Ethereum".into(),
            )
        } else {
            (
                format!("Zcash Account {}", index),
                "bip122:00040fe8ec8471911baa1f7c215a71e9".into(),
                format!("t1{:33}", index),
                "Zcash".into(),
            )
        };

        data.accounts.push(AccountInfo {
            account_id,
            name,
            chain_id,
            address,
            protocol,
        });
        Ok(())
    }

    async fn get_receive(&self, account_id: &str) -> ReceiveData {
        let data = self.data.lock().unwrap();
        let account = data
            .accounts
            .iter()
            .find(|a| a.account_id == account_id);

        match account {
            Some(acc) if acc.protocol == "Zcash" => ReceiveData {
                address: acc.address.clone(),
                chain_id: acc.chain_id.clone(),
                address_format: "transparent".into(),
                qr_payload: format!("zcash:{}", acc.address),
                account_id: account_id.to_string(),
            },
            Some(acc) => ReceiveData {
                address: acc.address.clone(),
                chain_id: acc.chain_id.clone(),
                address_format: "hex".into(),
                qr_payload: format!("ethereum:{}", acc.address),
                account_id: account_id.to_string(),
            },
            None => ReceiveData {
                address: "0x742d35Cc6634C0532925a3b844Bc454e4438f44e".into(),
                chain_id: "eip155:1".into(),
                address_format: "hex".into(),
                qr_payload: "ethereum:0x742d35Cc6634C0532925a3b844Bc454e4438f44e".into(),
                account_id: account_id.to_string(),
            },
        }
    }

    async fn submit_receive(&self, input: ReceiveInput) -> ReceiveData {
        if input.selected_chain_id.contains("bip122") {
            ReceiveData {
                address: "t1YhnKpPk6KxqGHgK7LKzK5qLpK5qLpK5qL".into(),
                chain_id: "bip122:00040fe8ec8471911baa1f7c215a71e9".into(),
                address_format: "transparent".into(),
                qr_payload: "zcash:t1YhnKpPk6KxqGHgK7LKzK5qLpK5qLpK5qL".into(),
                account_id: "acc_2".into(),
            }
        } else {
            self.get_receive("acc_1").await
        }
    }

    async fn get_send(&self, account_id: &str) -> SendData {
        let data = self.data.lock().unwrap();
        let account = data.accounts.iter().find(|a| a.account_id == account_id);

        match account {
            Some(acc) if acc.protocol == "Zcash" => SendData {
                account_id: account_id.to_string(),
                from_address: acc.address.clone(),
                spendable_balance: "500000000".into(),
                decimals: 8,
                chain_id: acc.chain_id.clone(),
            },
            Some(acc) => SendData {
                account_id: account_id.to_string(),
                from_address: acc.address.clone(),
                spendable_balance: "1420000000000000000".into(),
                decimals: 18,
                chain_id: acc.chain_id.clone(),
            },
            None => SendData {
                account_id: account_id.to_string(),
                from_address: "0x742d35Cc6634C0532925a3b844Bc454e4438f44e".into(),
                spendable_balance: "1420000000000000000".into(),
                decimals: 18,
                chain_id: "eip155:1".into(),
            },
        }
    }

    async fn submit_send_review(&self, input: SendReviewInput) -> SendReviewData {
        let fee_est = "409500000000000";
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
            nonce: 42,
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

    async fn receive_state(&self, account_id: &str) -> ApiState<ReceiveData> {
        let data = {
            let cache = self.receive_cache.lock().unwrap();
            cache.get(account_id).cloned()
        };
        if let Some(data) = data {
            return ApiState::Loaded(data);
        }
        let real = self.get_receive(account_id).await;
        self.receive_cache
            .lock()
            .unwrap()
            .insert(account_id.to_string(), real.clone());
        ApiState::Loaded(real)
    }

    async fn refresh_receive(&self, account_id: &str) {
        self.receive_cache.lock().unwrap().remove(account_id);
    }

    async fn send_state(&self, account_id: &str) -> ApiState<SendData> {
        let data = {
            let cache = self.send_cache.lock().unwrap();
            cache.get(account_id).cloned()
        };
        if let Some(data) = data {
            return ApiState::Loaded(data);
        }
        let real = self.get_send(account_id).await;
        self.send_cache
            .lock()
            .unwrap()
            .insert(account_id.to_string(), real.clone());
        ApiState::Loaded(real)
    }

    async fn refresh_send(&self, account_id: &str) {
        self.send_cache.lock().unwrap().remove(account_id);
    }
}
