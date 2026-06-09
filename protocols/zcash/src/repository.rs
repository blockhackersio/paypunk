use std::sync::{Arc, Mutex};

use paypunk_types::{Balance, WalletRepository};
use zcash_client_backend::data_api::{WalletRead, WalletWrite};
use zcash_client_sqlite::{util::SystemClock, WalletDb};
use zcash_protocol::local_consensus::LocalNetwork;

type ZcashWalletDb = WalletDb<SystemClock, LocalNetwork>;

pub struct ZcashWalletRepository {
    wallet_db: Arc<Mutex<ZcashWalletDb>>,
}

impl ZcashWalletRepository {
    pub fn new(wallet_db: ZcashWalletDb) -> Self {
        Self {
            wallet_db: Arc::new(Mutex::new(wallet_db)),
        }
    }

    pub fn from_arc(wallet_db: Arc<Mutex<ZcashWalletDb>>) -> Self {
        Self { wallet_db }
    }

    pub fn wallet_db(&self) -> &Arc<Mutex<ZcashWalletDb>> {
        &self.wallet_db
    }
}

impl WalletRepository for ZcashWalletRepository {
    fn get_balance(&self, account: u32) -> Result<Balance, String> {
        use zcash_client_backend::data_api::wallet::ConfirmationsPolicy;
        use zcash_protocol::value::Zatoshis;

        let db = self.wallet_db.lock().map_err(|e| e.to_string())?;
        let summary = db
            .get_wallet_summary(ConfirmationsPolicy::default())
            .map_err(|e| format!("get_wallet_summary failed: {e}"))?;

        let account_id =
            zip32::AccountId::try_from(account).map_err(|_| format!("invalid account: {account}"))?;

        match summary.and_then(|s| s.account_balances().get(&account_id).cloned()) {
            Some(bal) => {
                let total = u64::from(bal.total());
                let spendable = u64::from(bal.spendable_value());
                let pending = total.saturating_sub(spendable);
                Ok(Balance {
                    spendable: paypunk_types::Amount(spendable),
                    pending: paypunk_types::Amount(pending),
                    total: paypunk_types::Amount(total),
                })
            }
            None => Ok(Balance {
                spendable: paypunk_types::Amount(0),
                pending: paypunk_types::Amount(0),
                total: paypunk_types::Amount(0),
            }),
        }
    }

    fn get_spendable_resources(&self, _account: u32) -> Result<Vec<Vec<u8>>, String> {
        // For Zcash, note selection is handled by the InputSelector in
        // propose_transfer. Return empty vec — callers should use
        // propose_and_build instead of manual resource selection.
        Ok(Vec::new())
    }

    fn mark_resources_spent(&self, _account: u32, _txid: &str) -> Result<(), String> {
        // Note spentness is tracked automatically by
        // extract_and_store_transaction_from_pczt after a transfer.
        Ok(())
    }

    fn store_transaction(&self, _account: u32, _txid: &str, raw_tx: &[u8]) -> Result<(), String> {
        use zcash_primitives::transaction::Transaction;
        use zcash_protocol::consensus::BranchId;

        let tx = Transaction::read(raw_tx, BranchId::Nu6)
            .map_err(|e| format!("transaction deserialize failed: {e}"))?;

        let mut db = self.wallet_db.lock().map_err(|e| e.to_string())?;
        db.store_transaction(&tx)
            .map_err(|e| format!("store_transaction failed: {e}"))?;
        Ok(())
    }
}
