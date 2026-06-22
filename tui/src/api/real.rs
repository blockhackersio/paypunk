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
    derivation_path: Vec<u8>,
}

pub struct RealWalletApi {
    client: Client,
    pending: Mutex<Option<PendingSend>>,
    pending_mnemonic: Mutex<Option<Zeroizing<String>>>,
}

impl RealWalletApi {
    pub async fn connect(socket_path: &str) -> Result<Self, String> {
        let client = Client::connect(socket_path).await?;
        Ok(Self {
            client,
            pending: Mutex::new(None),
            pending_mnemonic: Mutex::new(None),
        })
    }

    pub fn with_client(client: Client) -> Self {
        Self {
            client,
            pending: Mutex::new(None),
            pending_mnemonic: Mutex::new(None),
        }
    }
}

fn parse_account_index(path: &str) -> u32 {
    path.rsplit('\'')
        .nth(1)
        .and_then(|s| s.split('/').last())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
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
                let caip10 = format!("eip155:1:{}", account.address);
                let balance = self
                    .client
                    .get_balance(caip10, "eip155:1/slip44:60".to_string())
                    .await
                    .map(|b| b.spendable.0.to_string())
                    .unwrap_or_else(|_| "0".to_string());
                AssetsData {
                    assets: vec![AssetRow {
                        name: "Ethereum".into(),
                        ticker: "ETH".into(),
                        price: "\u{2014}".into(),
                        price_change: "\u{2014}".into(),
                        price_change_up: true,
                        holdings_value: "\u{2014}".into(),
                        holdings_amount: balance,
                        chain_id: "eip155:1".into(),
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
                        let chain_id = match a.protocol {
                            ProtocolId::Ethereum => "eip155:1".to_string(),
                            _ => format!("{}:0", format!("{:?}", a.protocol).to_lowercase()),
                        };
                        AccountInfo {
                            account_id: a.id.clone(),
                            name: a.name.clone(),
                            address: a.address.clone(),
                            chain_id,
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
            .map(|a| AccountInfo {
                account_id: a.id.clone(),
                name: a.name.clone(),
                address: a.address.clone(),
                chain_id: "eip155:1".to_string(),
                protocol: format!("{:?}", a.protocol),
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
        let _ = self
            .client
            .create_account(
                ProtocolId::Ethereum,
                format!("m/44'/60'/{next_index}'"),
                next_index,
                format!("Ethereum Account {next_index}"),
            )
            .await
            .map_err(ApiError)?;
        Ok(())
    }

    async fn get_receive(&self, account_id: &str) -> ReceiveData {
        match self.client.get_account(account_id.to_string()).await {
            Ok(Some(account)) => ReceiveData {
                address: account.address.clone(),
                chain_id: "eip155:1".to_string(),
                address_format: "hex".to_string(),
                qr_payload: account.address,
                account_id: account_id.to_string(),
            },
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
                let caip10 = format!("eip155:1:{}", account.address);
                let balance = self
                    .client
                    .get_balance(caip10, "eip155:1/slip44:60".to_string())
                    .await
                    .map(|b| b.spendable.0.to_string())
                    .unwrap_or_else(|_| "0".to_string());
                SendData {
                    account_id: account_id.to_string(),
                    from_address: account.address,
                    spendable_balance: balance,
                    decimals: 18,
                    chain_id: "eip155:1".to_string(),
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

        let (from_address, derivation_path) = match &account {
            Some(a) => (a.address.clone(), a.derivation_path.clone()),
            None => (String::new(), String::new()),
        };

        let intent = Intent::Ethereum(EthereumIntent::Transfer {
            to: input.to_address.clone(),
            amount: input.amount.clone(),
            from: from_address,
            asset: "eip155:1/slip44:60".into(),
            data: None,
        });

        let account_index = parse_account_index(&derivation_path);
        let path_bytes = account_index.to_le_bytes();

        match self.client.submit_intent(intent, &path_bytes).await {
            Ok((raw_artifact, parsed_summary, keypunkd_signature, keypunkd_public_key)) => {
                let pending = PendingSend {
                    raw_artifact,
                    keypunkd_signature,
                    keypunkd_public_key,
                    derivation_path: path_bytes.to_vec(),
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
}
