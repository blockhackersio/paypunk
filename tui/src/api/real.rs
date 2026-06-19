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
    derivation_index: u32,
}

impl RealWalletApi {
    pub async fn connect(socket_path: &str) -> Result<Self, String> {
        let client = Client::connect(socket_path).await?;
        Ok(Self {
            client,
            pending: Mutex::new(None),
            derivation_index: 0,
        })
    }

    pub fn with_client(client: Client) -> Self {
        Self {
            client,
            pending: Mutex::new(None),
            derivation_index: 0,
        }
    }

    pub fn set_derivation_index(&mut self, index: u32) {
        self.derivation_index = index;
    }
}

#[async_trait(?Send)]
impl WalletApi for RealWalletApi {
    async fn get_setup(&self) -> SetupData {
        SetupData {
            app_version: "0.1.0".to_string(),
            wallet_exists: false,
            new_mnemonic: vec![],
            word_count: 12,
            import_methods: vec!["mnemonic".into()],
        }
    }

    async fn submit_setup_create(&self, input: SetupCreateInput) -> Result<(), ApiError> {
        self.client
            .generate_seed(Zeroizing::new(input.password))
            .await
            .map(|_| ())
            .map_err(|e| ApiError(e))
    }

    async fn submit_setup_import(&self, input: SetupImportInput) -> Result<(), ApiError> {
        self.client
            .restore_seed(Zeroizing::new(input.secret), Zeroizing::new(input.password))
            .await
            .map_err(|e| ApiError(e))
    }

    async fn get_wallets(&self) -> WalletsData {
        WalletsData { wallets: vec![] }
    }

    async fn get_assets(&self, chain_id: &str) -> AssetsData {
        if chain_id.contains("eip155") {
            AssetsData {
                assets: vec![AssetRow {
                    name: "Ethereum".into(),
                    ticker: "ETH".into(),
                    price: "$2,000.00".into(),
                    price_change: "▲ 0.00%".into(),
                    price_change_up: true,
                    holdings_value: "$0.00".into(),
                    holdings_amount: "0 ETH".into(),
                    chain_id: chain_id.into(),
                }],
            }
        } else {
            AssetsData { assets: vec![] }
        }
    }

    async fn get_home(&self) -> HomeData {
        HomeData {
            accounts: vec![],
            balances: vec![],
            total_fiat_value: 0.0,
            fiat_currency: "USD".into(),
            pending_tx: None,
        }
    }

    async fn submit_home(&self, _input: HomeInput) -> HomeData {
        self.get_home().await
    }

    async fn home_state(&self) -> ApiState<HomeData> {
        ApiState::Loaded(self.get_home().await)
    }

    async fn refresh_home(&self) {}

    async fn get_receive(&self, chain_id: &str) -> ReceiveData {
        ReceiveData {
            address: "not_derived".into(),
            chain_id: chain_id.into(),
            address_format: "hex".into(),
            qr_payload: String::new(),
        }
    }

    async fn submit_receive(&self, _input: ReceiveInput) -> ReceiveData {
        self.get_receive("").await
    }

    async fn receive_state(&self, chain_id: &str) -> ApiState<ReceiveData> {
        ApiState::Loaded(self.get_receive(chain_id).await)
    }

    async fn refresh_receive(&self, _chain_id: &str) {}

    async fn get_send(&self, chain_id: &str) -> SendData {
        let is_eth = chain_id.contains("eip155");
        SendData {
            from_address: "0x0000000000000000000000000000000000000000".into(),
            spendable_balance: "0".into(),
            decimals: if is_eth { 18 } else { 8 },
            chain_id: chain_id.into(),
            fee_data: if is_eth {
                FeeData::Eth(FeeDataEth {
                    base_fee_per_gas: "0".into(),
                    max_priority_fee_per_gas: "0".into(),
                    gas_limit_estimate: "21000".into(),
                })
            } else {
                FeeData::Zec(FeeRates { slow: 0, medium: 0, fast: 0 })
            },
            nonce: if is_eth { Some(0) } else { None },
            utxos: None,
        }
    }

    async fn submit_send_review(&self, input: SendReviewInput) -> SendReviewData {
        let intent = Intent::Ethereum(EthereumIntent::Transfer {
            to: input.to_address.clone(),
            amount: input.amount.clone(),
            from: "0x0000000000000000000000000000000000000000".into(),
            asset: input.token_id.clone(),
            data: None,
        });

        let path = self.derivation_index.to_le_bytes();

        match self.client.submit_intent(intent, &path).await {
            Ok((raw_artifact, parsed_summary, keypunkd_signature, keypunkd_public_key)) => {
                let pending = PendingSend {
                    raw_artifact,
                    keypunkd_signature,
                    keypunkd_public_key,
                    derivation_path: path.to_vec(),
                };
                *self.pending.lock().unwrap() = Some(pending);

                if let Ok(summary) = postcard::from_bytes::<ArtifactSummary>(&parsed_summary) {
                    SendReviewData {
                        to_address: summary.to,
                        amount: summary.amount.clone(),
                        fee_estimate: summary.fee,
                        total_amount: summary.amount,
                        chain_id: input.chain_id,
                    }
                } else {
                    SendReviewData {
                        to_address: input.to_address,
                        amount: input.amount.clone(),
                        fee_estimate: "unknown".into(),
                        total_amount: input.amount,
                        chain_id: input.chain_id,
                    }
                }
            }
            Err(e) => {
                SendReviewData {
                    to_address: format!("Error: {e}"),
                    amount: String::new(),
                    fee_estimate: String::new(),
                    total_amount: String::new(),
                    chain_id: input.chain_id,
                }
            }
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
                                block_explorer_url: format!(
                                    "https://etherscan.io/tx/{}",
                                    tx_hash
                                ),
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

    async fn send_state(&self, chain_id: &str) -> ApiState<SendData> {
        ApiState::Loaded(self.get_send(chain_id).await)
    }

    async fn refresh_send(&self, _chain_id: &str) {}

    async fn get_lock(&self) -> LockData {
        LockData {
            auth_methods: LockAuthMethods {
                biometric_available: false,
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
                biometric_enabled: false,
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
        Err(ApiError("reveal phrase not yet supported via real API".into()))
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
