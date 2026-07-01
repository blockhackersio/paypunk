use std::str::FromStr;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use rand_core::OsRng;
use tactix::{Actor, Ctx, Handler, Message};
use zcash_client_backend::data_api::error::Error as DataApiError;
use zcash_client_backend::data_api::wallet::propose_standard_transfer_to_address;
use zcash_client_backend::data_api::wallet::ConfirmationsPolicy;
use zcash_client_backend::data_api::WalletRead;
use zcash_client_backend::fees::StandardFeeRule;
use zcash_client_sqlite::error::SqliteClientError;
use zcash_client_sqlite::ReceivedNoteId;
use zcash_client_sqlite::util::SystemClock;
use zcash_client_sqlite::WalletDb;
use zcash_protocol::consensus::{BlockHeight, Network};
use zcash_protocol::memo::Memo;
use zcash_protocol::ShieldedProtocol;

use crate::lsp_client::LspClient;
use paypunk_types::SyncStatus;

/// Messages sent to the Zcash WalletDbActor.
#[derive(Debug, Message)]
#[response(Result<Vec<u8>, String>)]
pub enum WalletMessage {
    /// Build an unsigned PCZT for a transfer.
    ProposeAndBuild {
        public_key: Vec<u8>,
        account: u32,
        to: String,
        amount: u64,
        memo: Option<String>,
    },
    /// Trigger a chain sync from birthday height to latest.
    Sync {
        fvk: Vec<u8>,
        birthday_height: u64,
        lightwalletd_host: String,
    },
    /// Get the current sync status.
    GetStatus,
}

/// Tactix actor wrapping `zcash_client_sqlite::WalletDb` behind a Mutex.
pub struct WalletDbActor {
    pub db: Mutex<
        WalletDb<rusqlite::Connection, Network, SystemClock, OsRng>,
    >,
    pub params: Network,
    pub is_syncing: AtomicBool,
    pub current_height: AtomicU64,
    pub target_height: AtomicU64,
}

impl WalletDbActor {
    pub fn new(
        db: WalletDb<rusqlite::Connection, Network, SystemClock, OsRng>,
        params: Network,
    ) -> Self {
        Self {
            db: Mutex::new(db),
            params,
            is_syncing: AtomicBool::new(false),
            current_height: AtomicU64::new(0),
            target_height: AtomicU64::new(0),
        }
    }
}

impl Actor for WalletDbActor {}

impl Handler<WalletMessage> for WalletDbActor {
    async fn handle(&mut self, msg: WalletMessage, _ctx: &Ctx<Self>) -> Result<Vec<u8>, String> {
        match msg {
            WalletMessage::ProposeAndBuild {
                public_key,
                account: _account,
                to,
                amount,
                memo,
            } => {
                let _proposal = {
                    let mut db = self.db.lock().map_err(|e| e.to_string())?;

                    // Parse the recipient address
                    let to_addr = zcash_address::ZcashAddress::try_from_encoded(&to)
                        .map_err(|e| format!("invalid recipient address: {e}"))?;

                    let zcash_addr = to_addr
                        .convert()
                        .map_err(|e| format!("unsupported address type: {e}"))?;

                    // Parse the amount from zatoshis
                    let amount = zcash_protocol::value::Zatoshis::from_u64(amount)
                        .map_err(|_| "invalid amount".to_string())?;

                    // Deserialize the FVK from bytes
                    let fvk_bytes: [u8; 96] = public_key.as_slice().try_into()
                        .map_err(|_| "invalid FVK bytes: expected 96 bytes".to_string())?;
                    let _orchard_fvk = orchard::keys::FullViewingKey::from_bytes(&fvk_bytes)
                        .ok_or("invalid FVK")?;

                    // Get the first account from the wallet
                    let account_ids = db.get_account_ids()
                        .map_err(|e| format!("get_account_ids failed: {e}"))?;
                    let account_id = account_ids.first()
                        .ok_or("no accounts in wallet")?
                        .to_owned();

                    let memo = memo
                        .as_deref()
                        .map(Memo::from_str)
                        .transpose()
                        .map_err(|e| format!("invalid memo: {e}"))?
                        .map(zcash_protocol::memo::MemoBytes::from);

                    propose_standard_transfer_to_address::<
                        WalletDb<rusqlite::Connection, Network, SystemClock, OsRng>,
                        Network,
                        SqliteClientError,
                    >(
                        &mut *db,
                        &self.params,
                        StandardFeeRule::Zip317,
                        account_id,
                        ConfirmationsPolicy::MIN,
                        &zcash_addr,
                        amount,
                        memo,
                        None,
                        ShieldedProtocol::Orchard,
                    )
                    .map_err(|e: DataApiError<SqliteClientError, _, _, _, _, ReceivedNoteId>| {
                        format!("propose_transfer failed: {e}")
                    })
                }?;

                // TODO: Convert proposal to PCZT once create_pczt_from_proposal is available
                Err("ProposeAndBuild: PCZT creation from proposal not yet implemented".to_string())
            }
            WalletMessage::Sync {
                fvk: _fvk,
                birthday_height,
                lightwalletd_host,
            } => {
                if self.is_syncing.load(Ordering::SeqCst) {
                    return Err("sync already in progress".to_string());
                }

                self.is_syncing.store(true, Ordering::SeqCst);
                self.current_height.store(0, Ordering::SeqCst);

                // Connect to lightwalletd
                let mut lsp = LspClient::connect(&lightwalletd_host, self.params).await?;
                let latest = lsp.get_latest_height().await?;
                let latest_u64: u64 = latest.into();
                self.target_height.store(latest_u64, Ordering::SeqCst);

                let birthday = BlockHeight::from_u32(birthday_height as u32);

                // Scan blocks
                let (scanned_from, scanned_to) = {
                    let db = self.db.lock().map_err(|e| e.to_string())?;
                    lsp.scan_range(&*db, birthday, latest)?
                };

                self.current_height.store(scanned_to, Ordering::SeqCst);
                self.is_syncing.store(false, Ordering::SeqCst);

                let msg = format!("synced from block {} to {}", scanned_from, scanned_to);
                Ok(msg.into_bytes())
            }
            WalletMessage::GetStatus => {
                let status = SyncStatus {
                    is_syncing: self.is_syncing.load(Ordering::SeqCst),
                    current_height: self.current_height.load(Ordering::SeqCst),
                    target_height: self.target_height.load(Ordering::SeqCst),
                };
                postcard::to_allocvec(&status)
                    .map_err(|e| format!("serialize status failed: {e}"))
            }
        }
    }
}
