use std::collections::BTreeMap;

use orchard::keys::{FullViewingKey, SpendingKey};
use paypunk_types::{NonSignerProtocol, Protocol, ProtocolId, SignerProtocol, WalletRepository};
use pczt::roles::{
    combiner::Combiner, creator::Creator, io_finalizer::IoFinalizer, prover::Prover,
    signer::Signer, spend_finalizer::SpendFinalizer, tx_extractor::TransactionExtractor,
    updater::Updater, verifier::Verifier,
};
use zcash_keys::keys::UnifiedSpendingKey;
use zcash_primitives::transaction::builder::Builder;
use zcash_primitives::transaction::fees::zip317;
use zcash_protocol::consensus::{BlockHeight, Parameters};
use zip32::fingerprint::SeedFingerprint;

use crate::address;

pub struct ZcashProtocol;

impl Protocol for ZcashProtocol {
    fn protocol_id(&self) -> ProtocolId {
        ProtocolId::Zcash
    }

    fn derive_public_key(&self, seed: &[u8; 64], account: u32) -> Result<Vec<u8>, String> {
        SignerProtocol::derive_public_key(self, seed, account)
    }

    fn derive_address(&self, public_key: &[u8], index: u32) -> Result<String, String> {
        NonSignerProtocol::derive_address(self, public_key, index)
    }

    fn sign(&self, seed: &[u8; 64], account: u32, message: &[u8]) -> Result<Vec<u8>, String> {
        SignerProtocol::sign_transaction(self, seed, account, message)
    }
}

impl SignerProtocol for ZcashProtocol {
    fn protocol_id(&self) -> ProtocolId {
        ProtocolId::Zcash
    }

    fn derive_public_key(&self, seed: &[u8; 64], account: u32) -> Result<Vec<u8>, String> {
        let account_id =
            AccountId::try_from(account).map_err(|_| format!("invalid account: {account}"))?;
        let sk = SpendingKey::from_zip32_seed(seed, 133, account_id)
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
        let usk = UnifiedSpendingKey::from_seed(
            &zcash_protocol::consensus::Network::MainNetwork,
            seed,
            account_id,
        )
        .map_err(|e| format!("USK derivation failed: {e}"))?;

        let seed_fp = SeedFingerprint::from_seed(seed)
            .ok_or_else(|| "seed too short for fingerprint".to_string())?;
        let coin_type = zip32::ChildIndex::hardened(133);
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

        let mut signer =
            Signer::new(pczt).map_err(|e| format!("Signer::new failed: {e:?}"))?;

        for (_account_index, spends) in &keys {
            for keyref in spends {
                match keyref {
                    KeyRef::Orchard { index } => {
                        let ask = orchard::keys::SpendAuthorizingKey::from(usk.orchard());
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

impl NonSignerProtocol for ZcashProtocol {
    fn protocol_id(&self) -> ProtocolId {
        ProtocolId::Zcash
    }

    fn derive_address(&self, public_key: &[u8], index: u32) -> Result<String, String> {
        address::derive_from_fvk(public_key, index).map_err(|e| e.to_string())
    }

    fn propose_and_build(
        &self,
        _public_key: &[u8],
        _repository: &dyn WalletRepository,
        _account: u32,
        _to: &str,
        _amount: u64,
        _memo: Option<&str>,
    ) -> Result<Vec<u8>, String> {
        Err("propose_and_build requires a full wallet DB with notes — use Builder directly for test PCZTs".to_string())
    }

    fn prove_transaction(&self, transaction: &[u8]) -> Result<Vec<u8>, String> {
        let pczt =
            pczt::Pczt::parse(transaction).map_err(|e| format!("PCZT parse failed: {e:?}"))?;

        let pczt = Prover::new(pczt)
            .create_orchard_proof(&orchard::circuit::ProvingKey::build())
            .map_err(|e| format!("orchard proving failed: {e:?}"))?
            .finish();

        Ok(pczt.serialize())
    }

    fn finalize_transaction(
        &self,
        transaction: &[u8],
        signed_transaction: &[u8],
    ) -> Result<Vec<u8>, String> {
        let proven = pczt::Pczt::parse(transaction)
            .map_err(|e| format!("PCZT parse failed: {e:?}"))?;
        let signed = pczt::Pczt::parse(signed_transaction)
            .map_err(|e| format!("PCZT parse failed: {e:?}"))?;

        let combined = Combiner::new(vec![proven, signed])
            .combine()
            .map_err(|e| format!("combine failed: {e:?}"))?;

        let finalized = SpendFinalizer::new(combined)
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
}

enum KeyRef {
    Orchard { index: usize },
}

use zip32::AccountId;

/// Build a PCZT from scratch using the Builder, for testing purposes.
/// Creates a transaction with a transparent input and a transparent output.
pub fn build_test_pczt<P: Parameters>(
    params: &P,
    target_height: BlockHeight,
    transparent_inputs: Vec<zcash_transparent::builder::TransparentInputInfo>,
    to: &zcash_transparent::address::TransparentAddress,
    value: u64,
) -> Result<Vec<u8>, String> {
    let mut builder = Builder::new(
        params,
        target_height,
        zcash_primitives::transaction::builder::BuildConfig::Standard {
            sapling_anchor: None,
            orchard_anchor: None,
        },
    );

    for input in transparent_inputs {
        builder.add_transparent_input(input);
    }

    let zatoshis = zcash_protocol::value::Zatoshis::from_u64(value)
        .map_err(|e| format!("invalid amount: {e}"))?;
    builder
        .add_transparent_output(to, zatoshis)
        .map_err(|e| format!("add_transparent_output failed: {e}"))?;

    use rand_core::OsRng;
    let pczt_result = builder
        .build_for_pczt(OsRng, &zip317::FeeRule::standard())
        .map_err(|e| format!("build_for_pczt failed: {e}"))?;

    let created = Creator::build_from_parts(pczt_result.pczt_parts)
        .ok_or_else(|| "Tx version incompatible with PCZTs".to_string())?;

    let io_finalized = IoFinalizer::new(created)
        .finalize_io()
        .map_err(|e| format!("finalize_io failed: {e:?}"))?;

    let pczt = Updater::new(io_finalized).finish();
    Ok(pczt.serialize())
}
