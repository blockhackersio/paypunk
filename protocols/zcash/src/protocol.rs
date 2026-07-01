use std::collections::BTreeMap;

use async_trait::async_trait;
use paypunk_types::{
    ArtifactSummary, ChainId, Intent, Protocol, ProtocolId, SignerProtocol, ZcashIntent,
};
use pczt::roles::{
    signer::Signer, spend_finalizer::SpendFinalizer, tx_extractor::TransactionExtractor,
    verifier::Verifier,
};
use zcash_keys::keys::UnifiedSpendingKey;
use zcash_protocol::consensus::Parameters;
use zip32::fingerprint::SeedFingerprint;

use crate::wallet_client::ZcashWalletClient;

pub struct ZcashProtocol {
    pub params: zcash_protocol::consensus::Network,
    pub wallet_client: Option<ZcashWalletClient>,
    pub lightwalletd_host: Option<String>,
}

impl ZcashProtocol {
    pub const COIN_TYPE: u32 = 133;

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
        match self.params {
            zcash_protocol::consensus::Network::MainNetwork => ChainId {
                namespace: "zcash".to_string(),
                reference: "mainnet".to_string(),
            },
            zcash_protocol::consensus::Network::TestNetwork => ChainId {
                namespace: "zcash".to_string(),
                reference: "testnet".to_string(),
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
        let _pczt = pczt::Pczt::parse(artifact).map_err(|e| format!("PCZT parse failed: {e:?}"))?;

        // Extract information from the PCZT to build an ArtifactSummary
        // For now, extract what we can from the Orchard bundle
        let to = "Zcash address (see PCZT)".to_string();
        let amount = "0".to_string();
        let fee = "0".to_string();
        let memo = None;

        let summary = ArtifactSummary {
            to,
            amount,
            fee,
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

                let wallet = self.wallet_client.as_ref()
                    .ok_or_else(|| "WalletDb not initialized — sync required".to_string())?;

                let account = 0;

                let amount_f64: f64 = amount.parse().map_err(|_| "invalid amount".to_string())?;
                let amount_zat = (amount_f64 * 100_000_000.0) as u64;

                let public_key = vec![];

                wallet.create_transaction_async(
                    public_key,
                    account,
                    to.clone(),
                    amount_zat,
                    memo.clone(),
                ).await
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

    async fn get_balance(&self, _address: &str, _asset: &str) -> Result<paypunk_types::Balance, String> {
        let wallet = self.wallet_client.as_ref()
            .ok_or_else(|| "WalletDb not initialized — sync required".to_string())?;

        let _ = wallet;
        Err("get_balance via WalletDb not yet implemented".to_string())
    }

    async fn broadcast(&self, finalized_tx: &[u8]) -> Result<String, String> {
        let host = self.lightwalletd_host.as_ref()
            .ok_or_else(|| "lightwalletd not configured".to_string())?;

        let mut lsp = crate::lsp_client::LspClient::connect(host, self.params).await?;
        lsp.broadcast_tx(finalized_tx).await
    }

    // ── Protocol metadata ───────────────────────────────────────────────────

    fn chain_id(&self) -> ChainId {
        match self.params {
            zcash_protocol::consensus::Network::MainNetwork => ChainId {
                namespace: "zcash".to_string(),
                reference: "mainnet".to_string(),
            },
            zcash_protocol::consensus::Network::TestNetwork => ChainId {
                namespace: "zcash".to_string(),
                reference: "testnet".to_string(),
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
        let net = self.params.network_type();
        crate::address::derive_from_fvk(vk, index, net).map_err(|e| e.to_string())
    }
}

enum KeyRef {
    Orchard { index: usize },
}
