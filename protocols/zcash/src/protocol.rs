use std::collections::BTreeMap;

use orchard::keys::{FullViewingKey, SpendingKey};
use paypunk_types::{Protocol, ProtocolId, SignerProtocol};
use pczt::roles::{
    signer::Signer, spend_finalizer::SpendFinalizer,
    tx_extractor::TransactionExtractor, verifier::Verifier,
};
use zcash_keys::keys::UnifiedSpendingKey;
use zip32::fingerprint::SeedFingerprint;

use crate::address;

pub struct ZcashProtocol {
    pub params: zcash_protocol::consensus::Network,
}

impl ZcashProtocol {
    pub const COIN_TYPE: u32 = 133;
}

impl SignerProtocol for ZcashProtocol {
    fn protocol_id(&self) -> ProtocolId {
        ProtocolId::Zcash
    }

    fn derive_public_key(&self, seed: &[u8; 64], account: u32) -> Result<Vec<u8>, String> {
        let account_id =
            AccountId::try_from(account).map_err(|_| format!("invalid account: {account}"))?;
        let sk = SpendingKey::from_zip32_seed(seed, Self::COIN_TYPE, account_id)
            .map_err(|e| format!("ZIP 32 derivation failed: {e}"))?;
        let fvk = FullViewingKey::from(&sk);
        Ok(fvk.to_bytes().to_vec())
    }

    fn sign_transaction(
        &self,
        seed: &[u8; 64],
        account: u32,
        transaction: &[u8],
    ) -> Result<Vec<u8>, String> {
        let pczt =
            pczt::Pczt::parse(transaction).map_err(|e| format!("PCZT parse failed: {e:?}"))?;

        let account_id =
            AccountId::try_from(account).map_err(|_| format!("invalid account: {account}"))?;
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

impl Protocol for ZcashProtocol {
    fn protocol_id(&self) -> ProtocolId {
        ProtocolId::Zcash
    }

    fn derive_address(&self, public_key: &[u8], index: u32) -> Result<String, String> {
        address::derive_from_fvk(public_key, index).map_err(|e| e.to_string())
    }

    fn validate_address(&self, address: &str) -> bool {
        zcash_address::ZcashAddress::try_from_encoded(address).is_ok()
    }

    fn finalize_transaction(&self, transaction: &[u8]) -> Result<Vec<u8>, String> {
        let pczt =
            pczt::Pczt::parse(transaction).map_err(|e| format!("PCZT parse failed: {e:?}"))?;

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

    fn create_transaction(
        &self,
        _public_key: &[u8],
        _account: u32,
        _to: &str,
        _amount: u64,
        _memo: Option<&str>,
    ) -> Result<Vec<u8>, String> {
        // TODO: Build PCZT via zcash_primitives::Builder + zcash_client_backend
        // proposal APIs, then prove inline before returning. Proving is bundled
        // into creation because both need the same note/witness data and the
        // FullViewingKey is already available via public_key.
        //
        // Requires WalletDb for note selection and merkle paths.
        Err("create_transaction not yet implemented — needs WalletDb".to_string())
    }

    fn get_balance(&self, _account: u32, _public_key: &[u8]) -> Result<paypunk_types::Balance, String> {
        Err("get_balance not yet implemented — needs WalletDb + LSP chain scan".to_string())
    }
}

enum KeyRef {
    Orchard { index: usize },
}

use zip32::AccountId;
