use std::collections::BTreeMap;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;
use paypunk_types::{
    ArtifactSummary, BlockHeight, ChainId, HistoryEntry, Intent, Page, Protocol, ProtocolId,
    SignerProtocol, SyncStatus, TxStatus, ZcashIntent,
};
use pczt::roles::{
    prover::Prover, signer::Signer, spend_finalizer::SpendFinalizer,
    tx_extractor::TransactionExtractor, verifier::Verifier,
};
use tactix::{Addr, Recipient, Sender};
use tokio;
use zcash_keys::keys::UnifiedSpendingKey;
use zip32::fingerprint::SeedFingerprint;

use crate::scan_actor::SyncNewAccount;
use crate::wallet_actor::{
    EstimateFee, GetBalance, GetBlockHeight, GetHistory, GetStatus, GetTxStatus, ProposeAndBuild,
    RegisterAccount, StoreTransaction, WalletDbActor,
};

pub struct ZcashProtocol {
    pub params: zcash_protocol::consensus::Network,
    network_type: zcash_protocol::consensus::NetworkType,
    wallet_addr: Option<Addr<WalletDbActor>>,
    scan_recipient: Option<Arc<Recipient<SyncNewAccount>>>,
    pub lightwalletd_host: Option<String>,
    pub zcashd_rpc_url: Option<String>,
    address_viewing_keys: Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

impl ZcashProtocol {
    pub const COIN_TYPE: u32 = 133;

    pub fn new(
        params: zcash_protocol::consensus::Network,
        network_type: zcash_protocol::consensus::NetworkType,
        wallet_addr: Option<Addr<WalletDbActor>>,
        scan_recipient: Option<Recipient<SyncNewAccount>>,
        lightwalletd_host: Option<String>,
        zcashd_rpc_url: Option<String>,
    ) -> Self {
        Self {
            params,
            network_type,
            wallet_addr,
            scan_recipient: scan_recipient.map(Arc::new),
            lightwalletd_host,
            zcashd_rpc_url,
            address_viewing_keys: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn wallet_addr(&self) -> Option<Addr<WalletDbActor>> {
        self.wallet_addr.clone()
    }

    pub fn scan_recipient(&self) -> Option<Arc<Recipient<SyncNewAccount>>> {
        self.scan_recipient.clone()
    }

    /// Extract the account index from a BIP44-style derivation path.
    /// Expects format like `m/44'/133'/{account}'` and returns `{account}`.
    fn account_from_path(path: &str) -> Result<u32, String> {
        let account_str = path
            .rsplit('\'')
            .nth(1)
            .and_then(|s| s.split('/').last())
            .ok_or_else(|| format!("invalid derivation path: {path}"))?;
        account_str
            .parse()
            .map_err(|_| format!("invalid account index in path: {path}"))
    }
}

#[async_trait]
impl SignerProtocol for ZcashProtocol {
    async fn chain(&self) -> ChainId {
        match self.network_type {
            zcash_protocol::consensus::NetworkType::Main => ChainId {
                namespace: "zcash".to_string(),
                reference: "mainnet".to_string(),
            },
            zcash_protocol::consensus::NetworkType::Test => ChainId {
                namespace: "zcash".to_string(),
                reference: "testnet".to_string(),
            },
            zcash_protocol::consensus::NetworkType::Regtest => ChainId {
                namespace: "zcash".to_string(),
                reference: "regtest".to_string(),
            },
        }
    }

    fn export_viewing(&self, seed: &[u8; 64], path: &str) -> Result<Vec<u8>, String> {
        let account = Self::account_from_path(path)?;
        let account_id = zip32::AccountId::try_from(account)
            .map_err(|_| format!("invalid account: {account}"))?;
        let usk = UnifiedSpendingKey::from_seed(&self.params, seed, account_id)
            .map_err(|e| format!("USK derivation failed: {e}"))?;
        let fvk = usk.to_unified_full_viewing_key();
        let orchard_fvk = fvk.orchard().ok_or_else(|| "no Orchard FVK".to_string())?;
        Ok(orchard_fvk.to_bytes().to_vec())
    }

    fn parse_artifact(&self, artifact: &[u8]) -> Result<Vec<u8>, String> {
        let pczt = pczt::Pczt::parse(artifact).map_err(|e| format!("PCZT parse failed: {e:?}"))?;

        let (value_sum, negative) = pczt.orchard().value_sum();
        let fee = if *negative { 0u64 } else { *value_sum };

        // Extract information from the PCZT to build an ArtifactSummary
        let to = "Zcash address (see PCZT)".to_string();
        let amount = "0".to_string();
        let memo = None;

        let summary = ArtifactSummary {
            to,
            amount,
            fee: fee.to_string(),
            nonce: 0,
            memo,
            protocol: ProtocolId::Zcash,
        };

        postcard::to_allocvec(&summary).map_err(|e| format!("serialize summary failed: {e}"))
    }

    fn sign(&self, seed: &[u8; 64], path: &str, artifact: &[u8]) -> Result<Vec<u8>, String> {
        let account = Self::account_from_path(path)?;
        self.sign_transaction_inner(seed, account, artifact)
    }
}

impl ZcashProtocol {
    fn sign_transaction_inner(
        &self,
        seed: &[u8; 64],
        account: u32,
        transaction: &[u8],
    ) -> Result<Vec<u8>, String> {
        let pczt =
            pczt::Pczt::parse(transaction).map_err(|e| format!("PCZT parse failed: {e:?}"))?;

        let account_id = zip32::AccountId::try_from(account)
            .map_err(|_| format!("invalid account: {account}"))?;
        let usk = UnifiedSpendingKey::from_seed(&self.params, seed, account_id)
            .map_err(|e| format!("USK derivation failed: {e}"))?;

        let seed_fp = SeedFingerprint::from_seed(seed)
            .ok_or_else(|| "seed too short for fingerprint".to_string())?;
        let coin_type = zip32::ChildIndex::hardened(Self::COIN_TYPE);
        let mut keys: BTreeMap<zip32::AccountId, Vec<KeyRef>> = BTreeMap::new();

        let pczt = Verifier::new(pczt)
            .with_orchard::<std::convert::Infallible, _>(|bundle| {
                for (index, action) in bundle.actions().iter().enumerate() {
                    if let Some(account_idx) = action
                        .spend()
                        .zip32_derivation()
                        .as_ref()
                        .and_then(|d| d.extract_account_index(&seed_fp, coin_type))
                    {
                        keys.entry(account_idx)
                            .or_default()
                            .push(KeyRef::Orchard { index });
                    }
                }
                Ok(())
            })
            .map_err(|e| format!("Verifier::with_orchard failed: {e:?}"))?
            .finish();

        // Generate Orchard proof before signing
        let orchard_pk = orchard::circuit::ProvingKey::build();
        let pczt = Prover::new(pczt)
            .create_orchard_proof(&orchard_pk)
            .map_err(|e| format!("Prover::create_orchard_proof failed: {e:?}"))?
            .finish();

        let ask = orchard::keys::SpendAuthorizingKey::from(usk.orchard());

        if keys.is_empty() {
            let num_actions = pczt.orchard().actions().len();
            let mut signer = Signer::new(pczt).map_err(|e| format!("Signer::new failed: {e:?}"))?;
            for i in 0..num_actions {
                match signer.sign_orchard(i, &ask) {
                    Ok(()) => break,
                    Err(pczt::roles::signer::Error::InvalidIndex) => break,
                    Err(_) => continue,
                }
            }
            let pczt = signer.finish();
            return Ok(pczt.serialize());
        }

        let mut signer = Signer::new(pczt).map_err(|e| format!("Signer::new failed: {e:?}"))?;
        for (_account_index, spends) in &keys {
            for keyref in spends {
                match keyref {
                    KeyRef::Orchard { index } => {
                        signer
                            .sign_orchard(*index, &ask)
                            .map_err(|e| format!("sign_orchard failed: {e:?}"))?;
                    }
                }
            }
        }

        let pczt = signer.finish();
        Ok(pczt.serialize())
    }
}

#[async_trait]
impl Protocol for ZcashProtocol {
    fn protocol_id(&self) -> ProtocolId {
        ProtocolId::Zcash
    }

    async fn build(&self, intent: &Intent) -> Result<Vec<u8>, String> {
        match intent {
            Intent::Zcash(ZcashIntent::Transfer {
                to,
                amount,
                from,
                memo,
                ..
            }) => {
                if !self.validate_address(from) {
                    return Err(format!("invalid from address: {from}"));
                }

                let wallet = self
                    .wallet_addr
                    .as_ref()
                    .ok_or_else(|| "WalletDb not initialized — sync required".to_string())?;

                let account = 0;

                let amount_f64: f64 = amount.parse().map_err(|_| "invalid amount".to_string())?;
                let amount_zat = (amount_f64 * 100_000_000.0) as u64;

                wallet
                    .ask(ProposeAndBuild {
                        public_key: vec![],
                        account,
                        to: to.clone(),
                        amount: amount_zat,
                        memo: memo.clone(),
                    })
                    .await
            }
            _ => Err("unexpected intent variant for Zcash protocol".to_string()),
        }
    }

    fn validate_address(&self, address: &str) -> bool {
        zcash_address::ZcashAddress::try_from_encoded(address).is_ok()
    }

    fn finalize(&self, signed: &[u8]) -> Result<Vec<u8>, String> {
        let pczt = pczt::Pczt::parse(signed).map_err(|e| format!("PCZT parse failed: {e:?}"))?;

        let finalized = SpendFinalizer::new(pczt)
            .finalize_spends()
            .map_err(|e| format!("finalize_spends failed: {e:?}"))?;

        let orchard_vk = orchard::circuit::VerifyingKey::build();
        let tx = TransactionExtractor::new(finalized)
            .with_orchard(&orchard_vk)
            .extract()
            .map_err(|e| format!("extract failed: {e:?}"))?;

        let mut raw_tx = Vec::new();
        tx.write(&mut raw_tx)
            .map_err(|e| format!("tx serialize failed: {e}"))?;

        Ok(raw_tx)
    }

    async fn store_and_finalize(&self, signed_pczt: &[u8]) -> Result<Vec<u8>, String> {
        tracing::info!(
            "store_and_finalize: first bytes {:?} len={}",
            &signed_pczt[..signed_pczt.len().min(8)],
            signed_pczt.len()
        );
        // Store the transaction in the wallet DB
        if let Some(wallet) = &self.wallet_addr {
            wallet
                .ask(StoreTransaction {
                    pczt_bytes: signed_pczt.to_vec(),
                })
                .await?;
        }
        // Then finalize and return raw tx bytes
        self.finalize(signed_pczt)
    }

    async fn get_balance(
        &self,
        address: &str,
        _asset: &str,
    ) -> Result<paypunk_types::Balance, String> {
        let wallet = self
            .wallet_addr
            .as_ref()
            .ok_or_else(|| "WalletDb not initialized — sync required".to_string())?;

        let parsed = paypunk_types::caip::AccountId::parse(address)
            .map_err(|e| format!("invalid CAIP-10 address: {e}"))?;

        // Validate the raw Zcash address format before proceeding
        zcash_address::ZcashAddress::try_from_encoded(&parsed.account_address)
            .map_err(|e| format!("invalid Zcash address: {e}"))?;

        let viewing_key = {
            let map = self
                .address_viewing_keys
                .lock()
                .map_err(|e| e.to_string())?;
            map.get(&parsed.account_address).cloned()
        };

        let viewing_key = match viewing_key {
            Some(vk) => vk,
            None => {
                return Ok(paypunk_types::Balance {
                    spendable: paypunk_types::Amount(0),
                    pending: paypunk_types::Amount(0),
                    total: paypunk_types::Amount(0),
                });
            }
        };

        let balance = wallet.ask(GetBalance { viewing_key }).await?;
        Ok(balance)
    }

    async fn broadcast(&self, finalized_tx: &[u8]) -> Result<String, String> {
        let host = self
            .lightwalletd_host
            .as_ref()
            .ok_or_else(|| "lightwalletd not configured".to_string())?;

        let mut lsp = crate::lsp_client::LspClient::connect(host, self.params).await?;
        let result = lsp.broadcast_tx(finalized_tx).await?;

        // On regtest, mine a block so the transaction is confirmed immediately.
        if self.network_type == zcash_protocol::consensus::NetworkType::Regtest {
            if let Some(rpc_url) = &self.zcashd_rpc_url {
                tracing::info!("regtest: mining a block after broadcast");
                if let Err(e) = mine_block(rpc_url).await {
                    tracing::warn!(?e, "regtest mine_block failed (non-fatal)");
                }
            } else {
                tracing::warn!(
                    "regtest detected but no zcashd_rpc_url configured, skipping block mining"
                );
            }
        }

        Ok(result)
    }

    // ── Protocol metadata ───────────────────────────────────────────────────

    fn chain_id(&self) -> ChainId {
        match self.network_type {
            zcash_protocol::consensus::NetworkType::Main => ChainId {
                namespace: "zcash".to_string(),
                reference: "mainnet".to_string(),
            },
            zcash_protocol::consensus::NetworkType::Test => ChainId {
                namespace: "zcash".to_string(),
                reference: "testnet".to_string(),
            },
            zcash_protocol::consensus::NetworkType::Regtest => ChainId {
                namespace: "zcash".to_string(),
                reference: "regtest".to_string(),
            },
        }
    }

    fn native_asset(&self) -> String {
        match self.params {
            zcash_protocol::consensus::Network::MainNetwork => {
                "zcash:mainnet/slip44:133".to_string()
            }
            zcash_protocol::consensus::Network::TestNetwork => {
                "zcash:testnet/slip44:133".to_string()
            }
        }
    }

    fn ticker(&self) -> &str {
        "ZEC"
    }

    fn decimals(&self) -> u8 {
        8
    }

    fn block_explorer_url(&self, tx_hash: &str) -> String {
        match self.params {
            zcash_protocol::consensus::Network::MainNetwork => {
                format!("https://mainnet.zcashexplorer.app/tx/{}", tx_hash)
            }
            zcash_protocol::consensus::Network::TestNetwork => {
                format!("https://testnet.zcashexplorer.app/tx/{}", tx_hash)
            }
        }
    }

    fn default_derivation_path(&self, account: u32) -> String {
        crate::derivation_path(account)
    }

    fn default_account_name(&self, account_index: u32) -> String {
        format!("Zcash Account {account_index}")
    }

    // ── Key operations ──────────────────────────────────────────────────────

    fn derive_address_from_viewing_key(&self, vk: &[u8], index: u32) -> Result<String, String> {
        crate::address::derive_from_fvk(vk, index, self.network_type).map_err(|e| e.to_string())
    }

    // ── Chain sync ──────────────────────────────────────────────────────────

    async fn get_sync_status(&self) -> Result<SyncStatus, String> {
        let wallet = self
            .wallet_addr
            .as_ref()
            .ok_or_else(|| "WalletDb not initialized".to_string())?;
        let status = wallet.ask(GetStatus).await?;
        Ok(status)
    }

    // ── Transfer operations ──────────────────────────────────────────────────

    async fn create_transfer(
        &self,
        account: u32,
        to: String,
        amount: u64,
        memo: Option<String>,
    ) -> Result<Vec<u8>, String> {
        let wallet = self
            .wallet_addr
            .as_ref()
            .ok_or_else(|| "WalletDb not initialized".to_string())?;
        wallet
            .ask(ProposeAndBuild {
                public_key: vec![],
                account,
                to,
                amount,
                memo,
            })
            .await
    }

    async fn estimate_fee(
        &self,
        to: String,
        amount: u64,
        memo: Option<String>,
    ) -> Result<u64, String> {
        let wallet = self
            .wallet_addr
            .as_ref()
            .ok_or_else(|| "WalletDb not initialized".to_string())?;
        let fee: u64 = wallet
            .ask(EstimateFee { to, amount, memo })
            .await?;
        Ok(fee)
    }

    // ── History & status ────────────────────────────────────────────────────

    async fn get_history(
        &self,
        account: u32,
        cursor: Option<String>,
        limit: u32,
    ) -> Result<Page<HistoryEntry>, String> {
        let wallet = self
            .wallet_addr
            .as_ref()
            .ok_or_else(|| "WalletDb not initialized".to_string())?;
        let page: Page<HistoryEntry> = wallet
            .ask(GetHistory {
                account,
                cursor,
                limit,
            })
            .await?;
        Ok(page)
    }

    async fn get_transaction_status(&self, txid: String) -> Result<TxStatus, String> {
        let wallet = self
            .wallet_addr
            .as_ref()
            .ok_or_else(|| "WalletDb not initialized".to_string())?;
        let status: TxStatus = wallet.ask(GetTxStatus { txid }).await?;
        Ok(status)
    }

    async fn get_current_block_height(
        &self,
        lightwalletd_host: String,
    ) -> Result<BlockHeight, String> {
        let wallet = self
            .wallet_addr
            .as_ref()
            .ok_or_else(|| "WalletDb not initialized".to_string())?;
        let height: paypunk_types::BlockHeight = wallet
            .ask(GetBlockHeight { lightwalletd_host })
            .await?;
        Ok(height)
    }

    async fn sync_account(
        &self,
        viewing_key: &[u8],
        birthday_height: u64,
        address: &str,
    ) -> Result<(), String> {
        if viewing_key.len() != 96 {
            return Err(format!(
                "expected 96-byte Orchard FVK, got {} bytes",
                viewing_key.len(),
            ));
        }

        {
            let mut map = self
                .address_viewing_keys
                .lock()
                .map_err(|e| e.to_string())?;
            map.insert(address.to_string(), viewing_key.to_vec());
        }

        // Import FVK into the wallet DB (fast, non-blocking)
        let wallet = self
            .wallet_addr
            .as_ref()
            .ok_or_else(|| "WalletDb not initialized".to_string())?;
        let _ = wallet
            .ask(RegisterAccount {
                fvk: viewing_key.to_vec(),
                birthday_height,
            })
            .await?;

        // Trigger initial scan in the background (non-blocking for the caller)
        if let Some(scan) = &self.scan_recipient {
            let scan = scan.clone();
            tokio::spawn(async move {
                let _ = scan.ask(SyncNewAccount { birthday_height }).await;
            });
        }

        Ok(())
    }
}

enum KeyRef {
    Orchard { index: usize },
}

/// Mine a single block on regtest via zcashd JSON-RPC.
async fn mine_block(rpc_url: &str) -> Result<(), String> {
    let body = serde_json::json!({
        "jsonrpc": "1.0",
        "id": "paypunk",
        "method": "generate",
        "params": [1],
    });

    let client = reqwest::Client::builder()
        .build()
        .map_err(|e| format!("failed to build reqwest client: {e}"))?;

    let response = client
        .post(rpc_url)
        .basic_auth("zcashrpc", Some("notsecure"))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("zcashd RPC call failed: {e}"))?;

    let status = response.status();
    let text: String = response
        .text()
        .await
        .map_err(|e| format!("failed to read response: {e}"))?;

    if !status.is_success() {
        return Err(format!("zcashd RPC returned {status}: {text}"));
    }

    tracing::info!("regtest: block mined successfully");
    Ok(())
}
