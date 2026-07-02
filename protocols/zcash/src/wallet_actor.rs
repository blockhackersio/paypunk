use std::path::PathBuf;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::Mutex;

use rand_core::OsRng;
use tactix::{Actor, Ctx, Handler, Message};
use tracing::info;
use zcash_address::unified::{Encoding, Fvk, Ufvk};
use zcash_client_backend::data_api::chain::{
    error::Error as ChainError, scan_cached_blocks, BlockSource,
};
use zcash_client_backend::data_api::error::Error as DataApiError;
use zcash_client_backend::data_api::wallet::create_pczt_from_proposal;
use zcash_client_backend::data_api::wallet::propose_standard_transfer_to_address;
use zcash_client_backend::data_api::wallet::ConfirmationsPolicy;
use zcash_client_backend::data_api::{AccountBirthday, AccountPurpose, WalletRead, WalletWrite};
use zcash_client_backend::fees::StandardFeeRule;
use zcash_client_backend::proto::compact_formats::CompactBlock;
use zcash_client_backend::wallet::OvkPolicy;
use zcash_client_sqlite::error::SqliteClientError;
use zcash_client_sqlite::util::SystemClock;
use zcash_client_sqlite::ReceivedNoteId;
use zcash_client_sqlite::WalletDb;
use zcash_keys::keys::UnifiedFullViewingKey;
use zcash_protocol::consensus::{BlockHeight, Network};
use zcash_protocol::memo::Memo;
use zcash_protocol::ShieldedProtocol;

use crate::lsp_client::LspClient;
use paypunk_types::{Address, Amount, HistoryEntry, Page, SyncStatus, TxDirection, TxStatus};

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
    /// Get the balance for all accounts.
    GetBalance,
    /// Fetch transaction history for the given account.
    GetHistory {
        account: u32,
        cursor: Option<String>,
        limit: u32,
    },
    /// Get the current block height from lightwalletd.
    GetBlockHeight { lightwalletd_host: String },
    /// Get the status of a transaction by its txid.
    GetTxStatus { txid: String },
    /// Estimate the fee for a transfer.
    EstimateFee {
        to: String,
        amount: u64,
        memo: Option<String>,
    },
}

/// In-memory block source holding pre-fetched compact blocks.
struct VecBlockSource {
    blocks: Arc<Vec<CompactBlock>>,
}

impl BlockSource for VecBlockSource {
    type Error = String;

    fn with_blocks<F, WalletErrT>(
        &self,
        from_height: Option<BlockHeight>,
        limit: Option<usize>,
        mut with_block: F,
    ) -> Result<(), ChainError<WalletErrT, Self::Error>>
    where
        F: FnMut(CompactBlock) -> Result<(), ChainError<WalletErrT, Self::Error>>,
    {
        let from = from_height.map(u64::from).unwrap_or(0);
        let limit = limit.unwrap_or(usize::MAX);
        let mut count = 0;
        for block in self.blocks.iter() {
            let h = block.height;
            if h >= from && count < limit {
                with_block(block.clone()).map_err(|e| match e {
                    ChainError::Wallet(e) => ChainError::Wallet(e),
                    ChainError::BlockSource(e) => ChainError::BlockSource(e),
                    ChainError::Scan(e) => ChainError::Scan(e),
                })?;
                count += 1;
            }
        }
        Ok(())
    }
}

/// Tactix actor wrapping `zcash_client_sqlite::WalletDb` behind a Mutex.
pub struct WalletDbActor {
    pub db: Mutex<WalletDb<rusqlite::Connection, Network, SystemClock, OsRng>>,
    pub params: Network,
    pub is_syncing: AtomicBool,
    pub current_height: AtomicU64,
    pub target_height: AtomicU64,
    pub db_path: PathBuf,
}

impl WalletDbActor {
    pub fn new(
        db: WalletDb<rusqlite::Connection, Network, SystemClock, OsRng>,
        params: Network,
        db_path: PathBuf,
    ) -> Self {
        Self {
            db: Mutex::new(db),
            params,
            is_syncing: AtomicBool::new(false),
            current_height: AtomicU64::new(0),
            target_height: AtomicU64::new(0),
            db_path,
        }
    }
}

impl Actor for WalletDbActor {}

impl Handler<WalletMessage> for WalletDbActor {
    async fn handle(&mut self, msg: WalletMessage, _ctx: &Ctx<Self>) -> Result<Vec<u8>, String> {
        match msg {
            WalletMessage::ProposeAndBuild {
                public_key: _public_key,
                account: _account,
                to,
                amount,
                memo,
            } => {
                let mut db = self.db.lock().map_err(|e| e.to_string())?;

                let to_addr = zcash_address::ZcashAddress::try_from_encoded(&to)
                    .map_err(|e| format!("invalid recipient address: {e}"))?;

                let zcash_addr = to_addr
                    .convert()
                    .map_err(|e| format!("unsupported address type: {e}"))?;

                let amount_zat = zcash_protocol::value::Zatoshis::from_u64(amount)
                    .map_err(|_| "invalid amount".to_string())?;

                let account_ids = db
                    .get_account_ids()
                    .map_err(|e| format!("get_account_ids failed: {e}"))?;
                let account_id = account_ids
                    .first()
                    .ok_or("no accounts in wallet")?
                    .to_owned();

                let memo = memo
                    .as_deref()
                    .map(Memo::from_str)
                    .transpose()
                    .map_err(|e| format!("invalid memo: {e}"))?
                    .map(zcash_protocol::memo::MemoBytes::from);

                let proposal = propose_standard_transfer_to_address::<
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
                    amount_zat,
                    memo,
                    None,
                    ShieldedProtocol::Orchard,
                )
                .map_err(
                    |e: DataApiError<SqliteClientError, _, _, _, _, ReceivedNoteId>| {
                        format!("propose_transfer failed: {e}")
                    },
                )?;

                let pczt = create_pczt_from_proposal::<
                    WalletDb<rusqlite::Connection, Network, SystemClock, OsRng>,
                    Network,
                    SqliteClientError,
                    StandardFeeRule,
                    SqliteClientError,
                    ReceivedNoteId,
                >(
                    &mut *db,
                    &self.params,
                    account_id,
                    OvkPolicy::Sender,
                    &proposal,
                )
                .map_err(|e| format!("create_pczt_from_proposal failed: {e}"))?;

                Ok(pczt.serialize())
            }
            WalletMessage::Sync {
                fvk,
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

                // Parse the 96-byte Orchard FVK into a UnifiedFullViewingKey
                let fvk_bytes: [u8; 96] = fvk
                    .try_into()
                    .map_err(|_| "FVK must be 96 bytes".to_string())?;
                // Validate the FVK bytes are a valid Orchard FVK
                let _valid = orchard::keys::FullViewingKey::from_bytes(&fvk_bytes)
                    .ok_or("invalid Orchard FVK bytes")?;

                let ufvk_item = Fvk::Orchard(fvk_bytes);
                let ufvk_container = Ufvk::try_from_items(vec![ufvk_item])
                    .map_err(|e| format!("failed to build UFVK container: {e}"))?;
                let ufvk = UnifiedFullViewingKey::parse(&ufvk_container)
                    .map_err(|e| format!("failed to parse UFVK: {e}"))?;

                // Get tree state at birthday-1 for the ChainState
                let prev_height = if birthday > BlockHeight::from_u32(0) {
                    birthday - 1
                } else {
                    birthday
                };
                let tree_state = lsp.get_tree_state(prev_height).await?;
                let chain_state = tree_state
                    .to_chain_state()
                    .map_err(|e| format!("invalid tree state: {e}"))?;

                // Register the account in WalletDb
                {
                    let mut db = self.db.lock().map_err(|e| e.to_string())?;
                    let birthday = AccountBirthday::from_parts(chain_state.clone(), None);
                    db.import_account_ufvk(
                        "Zcash Account 0",
                        &ufvk,
                        &birthday,
                        AccountPurpose::Spending { derivation: None },
                        None,
                    )
                    .map_err(|e| format!("import_account_ufvk failed: {e}"))?;
                }

                // Update chain tip
                {
                    let mut db = self.db.lock().map_err(|e| e.to_string())?;
                    db.update_chain_tip(latest)
                        .map_err(|e| format!("update_chain_tip failed: {e}"))?;
                }

                // Fetch compact blocks from birthday to latest
                let blocks = lsp.get_block_range(birthday, latest).await?;
                let block_count = blocks.len();

                // Scan blocks using scan_cached_blocks
                let block_source = VecBlockSource {
                    blocks: Arc::new(blocks),
                };

                let _scan_summary = scan_cached_blocks(
                    &self.params,
                    &block_source,
                    &mut *self.db.lock().map_err(|e| e.to_string())?,
                    birthday,
                    &chain_state,
                    block_count,
                )
                .map_err(|e| format!("scan_cached_blocks failed: {e}"))?;

                let scanned_to = if birthday > latest {
                    u64::from(birthday)
                } else {
                    u64::from(latest)
                };
                self.current_height.store(scanned_to, Ordering::SeqCst);
                self.is_syncing.store(false, Ordering::SeqCst);

                let msg = format!(
                    "synced from block {} to {}",
                    u64::from(birthday),
                    scanned_to
                );
                Ok(msg.into_bytes())
            }
            WalletMessage::GetStatus => {
                let status = SyncStatus {
                    is_syncing: self.is_syncing.load(Ordering::SeqCst),
                    current_height: self.current_height.load(Ordering::SeqCst),
                    target_height: self.target_height.load(Ordering::SeqCst),
                };
                postcard::to_allocvec(&status).map_err(|e| format!("serialize status failed: {e}"))
            }
            WalletMessage::GetBalance => {
                info!("WalletMessage::GetBalance received by wallet actor");
                let db = self.db.lock().map_err(|e| e.to_string())?;
                let summary = db
                    .get_wallet_summary(ConfirmationsPolicy::MIN)
                    .map_err(|e| format!("get_wallet_summary failed: {e}"))?
                    .ok_or("wallet summary not available — sync first")?;

                // Sum Orchard balances across all accounts
                let mut total_spendable: u128 = 0;
                let mut total_pending: u128 = 0;
                let mut total_value: u128 = 0;
                info!("WalletMessage::GetBalance summing orchard balances...");
                for (_account_id, acct_balance) in summary.account_balances() {
                    let ob = acct_balance.orchard_balance();
                    let spendable: u64 = u64::from(ob.spendable_value());
                    let pending_change: u64 = u64::from(ob.change_pending_confirmation());
                    let pending_spendable: u64 = u64::from(ob.value_pending_spendability());
                    total_spendable += spendable as u128;
                    total_pending += (pending_change + pending_spendable) as u128;
                    total_value += (spendable + pending_change + pending_spendable) as u128;
                }

                info!(
                    "WalletMessage::GetBalance: spendable={}, pending={}, value={}",
                    total_spendable, total_pending, total_value
                );

                let balance = paypunk_types::Balance {
                    spendable: Amount(total_spendable),
                    pending: Amount(total_pending),
                    total: Amount(total_value),
                };

                postcard::to_allocvec(&balance)
                    .map_err(|e| format!("serialize balance failed: {e}"))
            }
            WalletMessage::GetHistory {
                account: _account,
                cursor: _cursor,
                limit: _limit,
            } => {
                let reader = rusqlite::Connection::open(&self.db_path)
                    .map_err(|e| format!("failed to open wallet db for reading: {e}"))?;

                let mut stmt = reader
                    .prepare(
                        "SELECT t.txid, t.block, t.expiry_height,
                            COALESCE(s.total_sent, 0) AS total_sent,
                            COALESCE(r.total_received, 0) AS total_received
                     FROM transactions t
                     LEFT JOIN (
                         SELECT tx, SUM(value) AS total_sent
                         FROM sent_notes GROUP BY tx
                     ) s ON s.tx = t.id_tx
                     LEFT JOIN (
                         SELECT tx, SUM(value) AS total_received
                         FROM received_notes GROUP BY tx
                     ) r ON r.tx = t.id_tx
                     ORDER BY t.id_tx DESC",
                    )
                    .map_err(|e| format!("prepare failed: {e}"))?;

                let tx_rows = stmt
                    .query_map([], |row| {
                        let txid_blob: Vec<u8> = row.get(0)?;
                        let block: Option<i64> = row.get(1)?;
                        let _expiry: Option<i64> = row.get(2)?;
                        let total_sent: i64 = row.get(3)?;
                        let total_received: i64 = row.get(4)?;
                        Ok((txid_blob, block, total_sent, total_received))
                    })
                    .map_err(|e| format!("query failed: {e}"))?;

                let mut entries: Vec<HistoryEntry> = Vec::new();
                for row in tx_rows {
                    let (txid_blob, block, total_sent, total_received) =
                        row.map_err(|e| format!("row error: {e}"))?;

                    let direction = if total_sent > 0 && total_received == 0 {
                        TxDirection::Outgoing
                    } else if total_received > 0 && total_sent == 0 {
                        TxDirection::Incoming
                    } else {
                        TxDirection::SelfTransfer
                    };

                    let amount = if direction == TxDirection::Outgoing {
                        Amount(total_sent as u128)
                    } else {
                        Amount(total_received as u128)
                    };

                    let status = match block {
                        Some(h) => TxStatus::Confirmed {
                            confirmations: h as u64,
                        },
                        None => TxStatus::Pending,
                    };

                    let hash = hex::encode(&txid_blob);
                    let timestamp = block.map(|h| h as u64);

                    entries.push(HistoryEntry {
                        hash,
                        direction,
                        counterparty: Address(String::new()),
                        amount,
                        status,
                        timestamp,
                    });
                }

                let page = Page {
                    items: entries,
                    next_cursor: None,
                    has_more: false,
                };
                postcard::to_allocvec(&page).map_err(|e| format!("serialize history failed: {e}"))
            }
            WalletMessage::GetBlockHeight { lightwalletd_host } => {
                let mut lsp = LspClient::connect(&lightwalletd_host, self.params).await?;
                let height = lsp.get_latest_height().await?;
                let height_u64: u64 = height.into();
                postcard::to_allocvec(&paypunk_types::BlockHeight(height_u64))
                    .map_err(|e| format!("serialize height failed: {e}"))
            }
            WalletMessage::GetTxStatus { txid } => {
                let reader = rusqlite::Connection::open(&self.db_path)
                    .map_err(|e| format!("failed to open wallet db: {e}"))?;

                let txid_bytes =
                    hex::decode(&txid).map_err(|e| format!("invalid txid hex: {e}"))?;

                let status = reader
                    .query_row(
                        "SELECT block FROM transactions WHERE txid = ?1",
                        rusqlite::params![txid_bytes],
                        |row| row.get::<_, Option<i64>>(0),
                    )
                    .map(|block| match block {
                        Some(h) => TxStatus::Confirmed {
                            confirmations: h as u64,
                        },
                        None => TxStatus::Pending,
                    })
                    .unwrap_or(TxStatus::NotFound);

                postcard::to_allocvec(&status).map_err(|e| format!("serialize status failed: {e}"))
            }
            WalletMessage::EstimateFee { to, amount, memo } => {
                let mut db = self.db.lock().map_err(|e| e.to_string())?;

                let to_addr = zcash_address::ZcashAddress::try_from_encoded(&to)
                    .map_err(|e| format!("invalid recipient address: {e}"))?;

                let zcash_addr = to_addr
                    .convert()
                    .map_err(|e| format!("unsupported address type: {e}"))?;

                let amount_zat = zcash_protocol::value::Zatoshis::from_u64(amount)
                    .map_err(|_| "invalid amount".to_string())?;

                let account_ids = db
                    .get_account_ids()
                    .map_err(|e| format!("get_account_ids failed: {e}"))?;
                let account_id = account_ids
                    .first()
                    .ok_or("no accounts in wallet")?
                    .to_owned();

                let memo = memo
                    .as_deref()
                    .map(Memo::from_str)
                    .transpose()
                    .map_err(|e| format!("invalid memo: {e}"))?
                    .map(zcash_protocol::memo::MemoBytes::from);

                let proposal = propose_standard_transfer_to_address::<
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
                    amount_zat,
                    memo,
                    None,
                    ShieldedProtocol::Orchard,
                )
                .map_err(
                    |e: DataApiError<SqliteClientError, _, _, _, _, ReceivedNoteId>| {
                        format!("propose_transfer failed: {e}")
                    },
                )?;

                let fee = proposal.steps().first().balance().fee_required();
                let fee_u64: u64 = u64::from(fee);
                postcard::to_allocvec(&fee_u64).map_err(|e| format!("serialize fee failed: {e}"))
            }
        }
    }
}
