use async_trait::async_trait;
use paypunk_api::Client;
use paypunk_types::{ArtifactSummary, EthereumIntent, Intent, ProtocolId};
use std::sync::Mutex;
use zeroize::Zeroizing;

use super::types::*;
use super::WalletApi;

struct PendingSend {
    raw_artifact: Vec<u8>,
    keypunkd_signature: Vec<u8>,
    keypunkd_public_key: [u8; 32],
    derivation_path: String,
}

pub struct RealWalletApi {
    client: Client,
    pending: Mutex<Option<PendingSend>>,
    pending_mnemonic: Mutex<Option<Zeroizing<String>>>,
    address_book_entries: Mutex<Vec<AddressBookEntry>>,
}

impl RealWalletApi {
    pub async fn connect(socket_path: &str) -> Result<Self, String> {
        let client = Client::connect(socket_path).await?;
        Ok(Self {
            client,
            pending: Mutex::new(None),
            pending_mnemonic: Mutex::new(None),
            address_book_entries: Mutex::new(Vec::new()),
        })
    }

    pub fn with_client(client: Client) -> Self {
        Self {
            client,
            pending: Mutex::new(None),
            pending_mnemonic: Mutex::new(None),
            address_book_entries: Mutex::new(Vec::new()),
        }
    }
}

fn format_balance(raw: &str, decimals: u8, ticker: &str) -> String {
    let divisor = 10u128.pow(decimals as u32) as f64;
    let value = raw.parse::<f64>().unwrap_or(0.0) / divisor;
    format!("{:.8} {}", value, ticker)
}

fn protocol_chain_asset(protocol: &ProtocolId) -> (&'static str, &'static str, u8, &'static str) {
    match protocol {
        ProtocolId::Ethereum => ("eip155:1", "eip155:1/slip44:60", 18, "ETH"),
        ProtocolId::Zcash => ("zcash:mainnet", "zcash:mainnet/slip44:133", 8, "ZEC"),
        _ => ("eip155:1", "eip155:1/slip44:60", 18, "ETH"),
    }
}

#[async_trait(?Send)]
impl WalletApi for RealWalletApi {
    async fn get_setup(&self) -> SetupData {
        let mnemonic = self.client.generate_mnemonic();
        let words: Vec<String> = mnemonic.split_whitespace().map(|s| s.to_string()).collect();
        *self.pending_mnemonic.lock().unwrap() = Some(mnemonic);
        SetupData {
            app_version: "0.1.0".to_string(),
            wallet_exists: false,
            new_mnemonic: words,
            word_count: 12,
            import_methods: vec!["mnemonic".into()],
        }
    }

    async fn submit_setup_create(&self, input: SetupCreateInput) -> Result<(), ApiError> {
        let mnemonic = self
            .pending_mnemonic
            .lock()
            .unwrap()
            .take()
            .ok_or_else(|| ApiError("no pending mnemonic — generate seed first".into()))?;
        self.client
            .restore_seed(mnemonic, Zeroizing::new(input.password.clone()))
            .await
            .map_err(|e| ApiError(e))?;
        self.client
            .unlock(Zeroizing::new(input.password))
            .await
            .map(|_| ())
            .map_err(|e| ApiError(e))
    }

    async fn submit_setup_import(&self, input: SetupImportInput) -> Result<(), ApiError> {
        self.client
            .restore_seed(
                Zeroizing::new(input.secret.clone()),
                Zeroizing::new(input.password.clone()),
            )
            .await
            .map_err(|e| ApiError(e))?;
        self.client
            .unlock(Zeroizing::new(input.password))
            .await
            .map(|_| ())
            .map_err(|e| ApiError(e))
    }

    async fn get_assets(&self, account_id: &str) -> AssetsData {
        match self.client.get_account(account_id.to_string()).await {
            Ok(Some(account)) => {
                let (chain, asset, decimals, ticker) =
                    protocol_chain_asset(&account.protocol);
                let caip10 = format!("{}:{}", chain, account.address);
                let balance = self
                    .client
                    .get_balance(caip10, asset.to_string())
                    .await
                    .map(|b| b.spendable.0.to_string())
                    .unwrap_or_else(|_| "0".to_string());
                let holdings = format_balance(&balance, decimals, ticker);
                AssetsData {
                    assets: vec![AssetRow {
                        name: ticker.to_string(),
                        ticker: ticker.to_string(),
                        price: "\u{2014}".into(),
                        price_change: "\u{2014}".into(),
                        price_change_up: true,
                        holdings_value: "\u{2014}".into(),
                        holdings_amount: holdings,
                        chain_id: chain.to_string(),
                    }],
                }
            }
            _ => AssetsData { assets: vec![] },
        }
    }

    async fn get_home(&self) -> HomeData {
        match self.client.list_accounts().await {
            Ok(accounts) => {
                let account_rows: Vec<AccountInfo> = accounts
                    .iter()
                    .map(|a| {
                        let (chain, _asset, _decimals, _ticker) =
                            protocol_chain_asset(&a.protocol);
                        AccountInfo {
                            account_id: a.id.clone(),
                            name: a.name.clone(),
                            address: a.address.clone(),
                            chain_id: chain.to_string(),
                            protocol: format!("{:?}", a.protocol),
                        }
                    })
                    .collect();
                HomeData {
                    accounts: account_rows,
                    fiat_currency: "USD".into(),
                }
            }
            Err(_) => HomeData {
                accounts: vec![],
                fiat_currency: "USD".into(),
            },
        }
    }

    async fn submit_home(&self, _input: HomeInput) -> HomeData {
        self.get_home().await
    }

    async fn home_state(&self) -> ApiState<HomeData> {
        ApiState::Loaded(self.get_home().await)
    }

    async fn refresh_home(&self) {}

    async fn list_accounts(&self) -> Result<Vec<AccountInfo>, ApiError> {
        let accounts = self.client.list_accounts().await.map_err(ApiError)?;
        Ok(accounts
            .iter()
            .map(|a| {
                let (chain, _asset, _decimals, _ticker) =
                    protocol_chain_asset(&a.protocol);
                AccountInfo {
                    account_id: a.id.clone(),
                    name: a.name.clone(),
                    address: a.address.clone(),
                    chain_id: chain.to_string(),
                    protocol: format!("{:?}", a.protocol),
                }
            })
            .collect())
    }

    async fn add_account(&self) -> Result<(), ApiError> {
        let accounts = self.client.list_accounts().await.map_err(ApiError)?;
        let eth_accounts: Vec<_> = accounts
            .iter()
            .filter(|a| a.protocol == ProtocolId::Ethereum)
            .collect();
        let next_index = eth_accounts.len() as u32;
        let path = self.client.derivation_path(ProtocolId::Ethereum, next_index);
        let _ = self
            .client
            .create_account(
                ProtocolId::Ethereum,
                path,
                next_index,
                format!("Ethereum Account {next_index}"),
            )
            .await
            .map_err(ApiError)?;
        Ok(())
    }

    async fn get_receive(&self, account_id: &str) -> ReceiveData {
        match self.client.get_account(account_id.to_string()).await {
            Ok(Some(account)) => {
                let (chain, _asset, _decimals, _ticker) =
                    protocol_chain_asset(&account.protocol);
                ReceiveData {
                    address: account.address.clone(),
                    chain_id: chain.to_string(),
                    address_format: "hex".to_string(),
                    qr_payload: account.address,
                    account_id: account_id.to_string(),
                }
            }
            _ => ReceiveData {
                address: "unknown".into(),
                chain_id: "eip155:1".into(),
                address_format: "hex".into(),
                qr_payload: String::new(),
                account_id: account_id.to_string(),
            },
        }
    }

    async fn submit_receive(&self, input: ReceiveInput) -> ReceiveData {
        self.get_receive(&input.selected_chain_id).await
    }

    async fn receive_state(&self, account_id: &str) -> ApiState<ReceiveData> {
        ApiState::Loaded(self.get_receive(account_id).await)
    }

    async fn refresh_receive(&self, _account_id: &str) {}

    async fn get_send(&self, account_id: &str) -> SendData {
        match self.client.get_account(account_id.to_string()).await {
            Ok(Some(account)) => {
                let (chain, asset, decimals, _ticker) =
                    protocol_chain_asset(&account.protocol);
                let caip10 = format!("{}:{}", chain, account.address);
                let balance = self
                    .client
                    .get_balance(caip10, asset.to_string())
                    .await
                    .map(|b| b.spendable.0.to_string())
                    .unwrap_or_else(|_| "0".to_string());
                SendData {
                    account_id: account_id.to_string(),
                    from_address: account.address,
                    spendable_balance: balance,
                    decimals,
                    chain_id: chain.to_string(),
                }
            }
            _ => SendData {
                account_id: account_id.to_string(),
                from_address: String::new(),
                spendable_balance: "0".to_string(),
                decimals: 18,
                chain_id: "eip155:1".to_string(),
            },
        }
    }

    async fn submit_send_review(&self, input: SendReviewInput) -> SendReviewData {
        let account = self
            .client
            .get_account(input.account_id.clone())
            .await
            .ok()
            .flatten();

        let (from_address, derivation_path, protocol) = match &account {
            Some(a) => (a.address.clone(), a.derivation_path.clone(), a.protocol),
            None => (String::new(), String::new(), ProtocolId::Ethereum),
        };

        let intent = match protocol {
            ProtocolId::Ethereum => Intent::Ethereum(EthereumIntent::Transfer {
                to: input.to_address.clone(),
                amount: input.amount.clone(),
                from: from_address,
                asset: "eip155:1/slip44:60".into(),
                data: None,
            }),
            ProtocolId::Zcash => Intent::Zcash(paypunk_types::ZcashIntent::Transfer {
                to: input.to_address.clone(),
                amount: input.amount.clone(),
                from: from_address,
                asset: "zcash:mainnet/slip44:133".into(),
                memo: None,
            }),
            _ => Intent::Ethereum(EthereumIntent::Transfer {
                to: input.to_address.clone(),
                amount: input.amount.clone(),
                from: from_address,
                asset: "eip155:1/slip44:60".into(),
                data: None,
            }),
        };

        match self.client.submit_intent(intent, &derivation_path).await {
            Ok((raw_artifact, parsed_summary, keypunkd_signature, keypunkd_public_key)) => {
                let pending = PendingSend {
                    raw_artifact,
                    keypunkd_signature,
                    keypunkd_public_key,
                    derivation_path: derivation_path,
                };
                *self.pending.lock().unwrap() = Some(pending);

                if let Ok(summary) = postcard::from_bytes::<ArtifactSummary>(&parsed_summary) {
                    SendReviewData {
                        to_address: summary.to,
                        amount: summary.amount.clone(),
                        fee_estimate: summary.fee,
                        total_amount: summary.amount,
                        chain_id: input.chain_id,
                        nonce: 0,
                    }
                } else {
                    SendReviewData {
                        to_address: input.to_address,
                        amount: input.amount.clone(),
                        fee_estimate: "unknown".into(),
                        total_amount: input.amount,
                        chain_id: input.chain_id,
                        nonce: 0,
                    }
                }
            }
            Err(e) => SendReviewData {
                to_address: format!("Error: {e}"),
                amount: String::new(),
                fee_estimate: String::new(),
                total_amount: String::new(),
                chain_id: input.chain_id,
                nonce: 0,
            },
        }
    }

    async fn submit_send_confirm(&self, input: SendConfirmInput) -> SendResult {
        let pending = self.pending.lock().unwrap().take();
        let password = input.auth_confirmation.value.clone();

        // Save recipient to address book
        let to_addr = input.reviewed.to_address.clone();
        let _ = self.add_address_book_entry(
            format!("Sent to {}", &to_addr[..to_addr.len().min(20)]),
            to_addr,
            "Ethereum".into(),
        ).await;
        match pending {
            Some(p) => {
                match self
                    .client
                    .approve_signature(
                        &p.raw_artifact,
                        &p.keypunkd_signature,
                        Zeroizing::new(password),
                        &p.derivation_path,
                    )
                    .await
                {
                    Ok(signed_artifact) => {
                        match self
                            .client
                            .broadcast_transaction(ProtocolId::Ethereum, signed_artifact)
                            .await
                        {
                            Ok(tx_hash) => SendResult {
                                tx_hash: tx_hash.clone(),
                                status: "broadcasted".into(),
                                block_explorer_url: format!("https://etherscan.io/tx/{}", tx_hash),
                            },
                            Err(e) => SendResult {
                                tx_hash: String::new(),
                                status: format!("broadcast failed: {e}"),
                                block_explorer_url: String::new(),
                            },
                        }
                    }
                    Err(e) => SendResult {
                        tx_hash: String::new(),
                        status: format!("signing failed: {e}"),
                        block_explorer_url: String::new(),
                    },
                }
            }
            None => SendResult {
                tx_hash: String::new(),
                status: "error: no pending transaction".into(),
                block_explorer_url: String::new(),
            },
        }
    }

    async fn send_state(&self, account_id: &str) -> ApiState<SendData> {
        ApiState::Loaded(self.get_send(account_id).await)
    }

    async fn refresh_send(&self, _account_id: &str) {}

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
            app_version: "0.1.0".into(),
        }
    }

    async fn submit_settings(&self, _input: SettingsInput) -> Result<(), ApiError> {
        Ok(())
    }

    async fn submit_reveal_phrase(
        &self,
        _input: RevealPhraseInput,
    ) -> Result<Vec<String>, ApiError> {
        Err(ApiError(
            "reveal phrase not yet supported via real API".into(),
        ))
    }

    async fn check_wallet_exists(&self) -> bool {
        self.client.check_wallet_exists().await.unwrap_or(false)
    }

    async fn unlock(&self, password: String) -> Result<UnlockData, ApiError> {
        self.client
            .unlock(Zeroizing::new(password))
            .await
            .map(|accounts_count| UnlockData { accounts_count })
            .map_err(|e| ApiError(e))
    }

    async fn get_address_book(&self) -> AddressBookData {
        let mut entries = self.address_book_entries.lock().unwrap().clone();

        // Populate from wallet accounts
        if let Ok(accounts) = self.client.list_accounts().await {
            for acc in &accounts {
                let (_chain, _asset, _decimals, ticker) =
                    protocol_chain_asset(&acc.protocol);
                let exists = entries.iter().any(|e| e.address == acc.address);
                if !exists {
                    entries.push(AddressBookEntry {
                        name: format!("{} ({})", acc.name, ticker),
                        address: acc.address.clone(),
                        protocol: format!("{:?}", acc.protocol),
                    });
                }
            }
        }

        AddressBookData { entries }
    }

    async fn add_address_book_entry(&self, name: String, address: String, protocol: String) {
        let mut entries = self.address_book_entries.lock().unwrap();
        let exists = entries.iter().any(|e| e.address == address);
        if !exists {
            entries.push(AddressBookEntry {
                name,
                address,
                protocol,
            });
        }
    }
}
