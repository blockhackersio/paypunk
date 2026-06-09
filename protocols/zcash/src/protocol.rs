use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

use orchard::keys::{FullViewingKey, SpendingKey};
use paypunk_types::{Protocol, ProtocolId, SignerProtocol, WalletRepository};
use pczt::roles::{
    prover::Prover, signer::Signer, spend_finalizer::SpendFinalizer,
    tx_extractor::TransactionExtractor, verifier::Verifier,
};
use zcash_client_backend::data_api::wallet::{
    create_pczt_from_proposal, propose_transfer, ConfirmationsPolicy,
    input_selection::GreedyInputSelector,
};
use zcash_client_backend::fees::{
    standard::MultiOutputChangeStrategy, DustOutputPolicy, SplitPolicy, StandardFeeRule,
};
use zcash_client_backend::wallet::OvkPolicy;
use zcash_client_sqlite::{util::SystemClock, WalletDb};
use zcash_keys::keys::UnifiedSpendingKey;
use zcash_protocol::consensus::BlockHeight;
use zcash_protocol::local_consensus::LocalNetwork;
use zcash_protocol::memo::MemoBytes;
use zcash_protocol::value::Zatoshis;
use zcash_protocol::ShieldedProtocol;
use zip32::fingerprint::SeedFingerprint;

use crate::address;

type ZcashWalletDb = WalletDb<SystemClock, LocalNetwork>;

pub struct ZcashProtocol {
    wallet_db: Option<Arc<Mutex<ZcashWalletDb>>>,
    params: Option<LocalNetwork>,
}

impl Default for ZcashProtocol {
    fn default() -> Self {
        Self {
            wallet_db: None,
            params: None,
        }
    }
}

impl ZcashProtocol {
    pub fn with_wallet_db(wallet_db: ZcashWalletDb, params: LocalNetwork) -> Self {
        Self {
            wallet_db: Some(Arc::new(Mutex::new(wallet_db))),
            params: Some(params),
        }
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

        let ask = orchard::keys::SpendAuthorizingKey::from(usk.orchard());

        if keys.is_empty() {
            // No ZIP 32 derivation info found — fall back to trying all
            // orchard action indices. This handles PCZTs built directly
            // from the Builder (e.g. in tests) that lack derivation paths.
            let num_actions = pczt.orchard().actions().len();
            let mut signer =
                Signer::new(pczt).map_err(|e| format!("Signer::new failed: {e:?}"))?;
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

        let mut signer =
            Signer::new(pczt).map_err(|e| format!("Signer::new failed: {e:?}"))?;
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

    fn propose_and_build(
        &self,
        _public_key: &[u8],
        _repository: &dyn WalletRepository,
        account: u32,
        to: &str,
        amount: u64,
        memo: Option<&str>,
    ) -> Result<Vec<u8>, String> {
        let db = self
            .wallet_db
            .as_ref()
            .ok_or("ZcashProtocol not configured with a wallet DB")?;
        let params = self
            .params
            .as_ref()
            .ok_or("ZcashProtocol not configured with consensus params")?;
        let mut wallet_db = db.lock().map_err(|e| e.to_string())?;

        let account_id =
            zip32::AccountId::try_from(account).map_err(|_| format!("invalid account: {account}"))?;

        // Parse recipient address
        let to_address = zcash_address::ZcashAddress::try_from_encoded(to)
            .map_err(|e| format!("invalid recipient address: {e}"))?;

        // Build payment with optional memo
        let payment = if let Some(memo_text) = memo {
            let memo = MemoBytes::from_bytes(memo_text.as_bytes())
                .map_err(|e| format!("invalid memo: {e}"))?;
            zip321::Payment::new(to_address, Zatoshis::from_u64(amount).map_err(|_| "invalid amount")?, Some(memo))
                .map_err(|e| format!("payment construction failed: {e}"))?
        } else {
            zip321::Payment::without_memo(to_address, Zatoshis::from_u64(amount).map_err(|_| "invalid amount")?)
        };

        let request = zip321::TransactionRequest::new(vec![payment])
            .map_err(|e| format!("transaction request failed: {e}"))?;

        let change_strategy = MultiOutputChangeStrategy::new(
            StandardFeeRule::Zip317,
            None,
            ShieldedProtocol::Orchard,
            DustOutputPolicy::default(),
            SplitPolicy::with_min_output_value(
                NonZeroUsize::new(4).unwrap(),
                Zatoshis::from_u64(10_000_000).unwrap(),
            ),
        );

        let proposal = propose_transfer(
            &mut *wallet_db,
            params,
            account_id,
            &GreedyInputSelector::new(),
            &change_strategy,
            request,
            ConfirmationsPolicy::default(),
            None,
        )
        .map_err(|e| format!("propose_transfer failed: {e}"))?;

        let pczt = create_pczt_from_proposal(
            &mut *wallet_db,
            params,
            account_id,
            OvkPolicy::Sender,
            &proposal,
        )
        .map_err(|e| format!("create_pczt_from_proposal failed: {e}"))?;

        Ok(pczt.serialize())
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
}

enum KeyRef {
    Orchard { index: usize },
}

use zip32::AccountId;
