use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use rand_core::OsRng;
use tactix::{Actor, Ctx, Handler, Message};
use tracing::info;
use zcash_address::unified::{Encoding, Fvk, Ufvk};
use zcash_client_backend::data_api::chain::{
    error::Error as ChainError, scan_cached_blocks, BlockSource,
};
use zcash_client_backend::data_api::error::Error as DataApiError;
use zcash_client_backend::data_api::wallet::create_pczt_from_proposal;
use zcash_client_backend::data_api::wallet::extract_and_store_transaction_from_pczt;
use zcash_client_backend::data_api::wallet::propose_standard_transfer_to_address;
use zcash_client_backend::data_api::wallet::ConfirmationsPolicy;
use zcash_client_backend::data_api::{
    Account, AccountBirthday, AccountPurpose, WalletRead, WalletWrite,
};
use zcash_client_backend::fees::StandardFeeRule;
use zcash_client_backend::proto::compact_formats::CompactBlock;
use zcash_client_backend::wallet::OvkPolicy;
use zcash_client_sqlite::error::SqliteClientError;
use zcash_client_sqlite::util::SystemClock;
use zcash_client_sqlite::AccountUuid;
use zcash_client_sqlite::ReceivedNoteId;
use zcash_client_sqlite::WalletDb;
use zcash_keys::keys::UnifiedFullViewingKey;
use zcash_protocol::consensus::{BlockHeight, Network, NetworkType};
use zcash_protocol::local_consensus::LocalNetwork;
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
    /// Register a new account (parse FVK, get tree state, import into WalletDb,
    /// store in self.accounts, and do initial full sync from birthday to tip).
    RegisterAccount {
        fvk: Vec<u8>,
        birthday_height: u64,
    },
    /// Incremental sync from current chain tip using stored accounts.
    /// No-op if no accounts registered.
    Sync,
    /// Get the current sync status.
    GetStatus,
    /// Get the balance for a specific UFVK.
    GetBalance { viewing_key: Vec<u8> },
    /// Fetch transaction history for the given account.
    GetHistory {
        account: u32,
        cursor: Option<String>,
        limit: u32,
    },
    /// Get the current block height from lightwalletd.
    GetBlockHeight { lightwalletd_host: String },
    /// Extract and store a signed PCZT in the wallet DB, returning raw tx bytes.
    StoreTransaction {
        pczt_bytes: Vec<u8>,
    },
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

/// Tactix actor wrapping `zcash_client_sqlite::WalletDb`.
pub struct WalletDbActor {
    pub db: WalletDb<rusqlite::Connection, Network, SystemClock, OsRng>,
    pub params: Network,
    pub network_type: NetworkType,
    pub is_syncing: AtomicBool,
    pub current_height: AtomicU64,
    pub target_height: AtomicU64,
    pub db_path: PathBuf,
    fvk_to_account_id: HashMap<Vec<u8>, AccountUuid>,
    confirmations_policy: ConfirmationsPolicy,
    lightwalletd_host: String,
    accounts: Vec<(Vec<u8>, u64)>,
}

impl WalletDbActor {
    pub fn new(
        db: WalletDb<rusqlite::Connection, Network, SystemClock, OsRng>,
        params: Network,
        network_type: NetworkType,
        db_path: PathBuf,
        confirmations_policy: ConfirmationsPolicy,
        lightwalletd_host: String,
    ) -> Self {
        Self {
            db,
            params,
            network_type,
            is_syncing: AtomicBool::new(false),
            current_height: AtomicU64::new(0),
            target_height: AtomicU64::new(0),
            db_path,
            fvk_to_account_id: HashMap::new(),
            confirmations_policy,
            lightwalletd_host,
            accounts: Vec::new(),
        }
    }

    /// Return the appropriate consensus parameters for transaction building.
    /// On regtest, all network upgrades activate at block 1, matching the
    /// `zcash.conf` used by the local regtest stack.
    fn build_params(&self) -> LocalNetwork {
        LocalNetwork {
            overwinter: Some(BlockHeight::from_u32(1)),
            sapling: Some(BlockHeight::from_u32(1)),
            blossom: Some(BlockHeight::from_u32(1)),
            heartwood: Some(BlockHeight::from_u32(1)),
            canopy: Some(BlockHeight::from_u32(1)),
            nu5: Some(BlockHeight::from_u32(1)),
            nu6: Some(BlockHeight::from_u32(1)),
            nu6_1: Some(BlockHeight::from_u32(1)),
        }
    }

    /// If the wallet DB file was deleted (e.g. by `paypunk reset` while the
    /// daemon is running), reinitialize the connection so writes don't go to
    /// an orphaned inode.
    fn ensure_db_file_exists(&mut self) -> Result<(), String> {
        if !self.db_path.exists() {
            tracing::warn!("wallet DB file deleted, reinitializing");
            if let Some(parent) = self.db_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("failed to create wallet db dir: {e}"))?;
            }
            let mut new_db = zcash_client_sqlite::WalletDb::for_path(
                &self.db_path,
                self.params,
                zcash_client_sqlite::util::SystemClock,
                rand_core::OsRng,
            )
            .map_err(|e| format!("failed to recreate wallet db: {e}"))?;
            zcash_client_sqlite::wallet::init::init_wallet_db(&mut new_db, None)
                .map_err(|e| format!("failed to init wallet db: {e}"))?;
            self.db = new_db;
        }
        Ok(())
    }

    async fn register_account(
        &mut self,
        fvk: Vec<u8>,
        birthday_height: u64,
    ) -> Result<String, String> {
        // If the DB file was deleted out from under us, reinitialize first.
        self.ensure_db_file_exists()?;

        info!("register_account: connecting to lightwalletd at {}", self.lightwalletd_host);
        let mut lsp = LspClient::connect(&self.lightwalletd_host, self.params).await?;
        info!("register_account: connected, getting latest height");
        let latest = lsp.get_latest_height().await?;
        let latest_u64: u64 = latest.into();
        self.target_height.store(latest_u64, Ordering::SeqCst);
        info!("register_account: latest height = {latest_u64}");

        let birthday = if birthday_height == 0 {
            info!("register_account: birthday_height is 0, defaulting to block 2");
            BlockHeight::from_u32(2)
        } else {
            BlockHeight::from_u32(birthday_height as u32)
        };

        info!("register_account: parsing 96-byte Orchard FVK");
        let fvk_bytes: [u8; 96] = fvk
            .try_into()
            .map_err(|_| "FVK must be 96 bytes".to_string())?;
        let _valid = orchard::keys::FullViewingKey::from_bytes(&fvk_bytes)
            .ok_or("invalid Orchard FVK bytes")?;

        let ufvk_item = Fvk::Orchard(fvk_bytes);
        let ufvk_container = Ufvk::try_from_items(vec![ufvk_item])
            .map_err(|e| format!("failed to build UFVK container: {e}"))?;
        let ufvk = UnifiedFullViewingKey::parse(&ufvk_container)
            .map_err(|e| format!("failed to parse UFVK: {e}"))?;

        let prev_height = if birthday > BlockHeight::from_u32(0) {
            birthday - 1
        } else {
            birthday
        };
        info!("register_account: getting tree state at height {prev_height:?}");
        let tree_state = lsp.get_tree_state(prev_height).await?;
        let chain_state = tree_state
            .to_chain_state()
            .map_err(|e| format!("invalid tree state: {e}"))?;

        {
            let account_name = format!("Zcash Account {}", self.fvk_to_account_id.len());
            let account_birthday = AccountBirthday::from_parts(chain_state.clone(), None);

            let account_uuid = match self.db.import_account_ufvk(
                &account_name,
                &ufvk,
                &account_birthday,
                AccountPurpose::Spending { derivation: None },
                None,
            ) {
                Ok(acct) => {
                    info!("register_account: imported UFVK as '{account_name}'");
                    acct.id()
                }
                Err(e) => {
                    info!("register_account: UFVK import skipped (already registered?): {e}");
                    let acct = self
                        .db
                        .get_account_for_ufvk(&ufvk)
                        .map_err(|e| format!("failed to query account by UFVK: {e}"))?
                        .ok_or_else(|| "account not found after import".to_string())?;
                    acct.id()
                }
            };

            self.fvk_to_account_id
                .insert(fvk_bytes.to_vec(), account_uuid);
        }

        // Store account for incremental sync (use adjusted birthday, not raw input)
        self.accounts.push((fvk_bytes.to_vec(), u64::from(birthday)));

        // Do initial full sync from birthday to tip
        info!("register_account: fetching blocks from {birthday:?} to {latest:?}");
        let blocks = lsp.get_block_range(birthday, latest).await?;
        let block_count = blocks.len();
        info!("register_account: fetched {block_count} blocks");

        let block_source = VecBlockSource {
            blocks: Arc::new(blocks),
        };

        info!("register_account: scanning {block_count} blocks");
        let _scan_summary = scan_cached_blocks(
            &self.params,
            &block_source,
            &mut self.db,
            birthday,
            &chain_state,
            block_count,
        )
        .map_err(|e| format!("scan_cached_blocks failed: {e}"))?;

        {
            info!("register_account: updating chain tip to {latest:?}");
            self.db
                .update_chain_tip(latest)
                .map_err(|e| format!("update_chain_tip failed: {e}"))?;
        }

        info!("register_account: scan complete");
        let msg = format!(
            "registered account, synced from block {} to {}",
            u64::from(birthday),
            latest_u64
        );
        Ok(msg)
    }

    async fn sync_from_tip(&mut self) -> Result<String, String> {
        if self.accounts.is_empty() {
            return Ok("no accounts registered, skipping sync".to_string());
        }

        self.ensure_db_file_exists()?;

        info!("sync_from_tip: connecting to lightwalletd at {}", self.lightwalletd_host);
        let mut lsp = LspClient::connect(&self.lightwalletd_host, self.params).await?;
        let latest = lsp.get_latest_height().await?;
        let latest_u64: u64 = latest.into();
        self.target_height.store(latest_u64, Ordering::SeqCst);
        info!("sync_from_tip: latest height = {latest_u64}");

        // Get current chain tip from wallet DB
        let chain_tip = self
            .db
            .chain_height()
            .map_err(|e| format!("chain_height failed: {e}"))?;

        let from_height = match chain_tip {
            Some(tip) => {
                // Start from the next block after the current tip
                let next = tip + 1;
                info!("sync_from_tip: current chain tip is {tip:?}, fetching from {next:?}");
                next
            }
            None => {
                info!("sync_from_tip: no chain tip, nothing to sync");
                self.current_height.store(latest_u64, Ordering::SeqCst);
                return Ok("no chain tip, nothing to sync".to_string());
            }
        };

        if from_height > latest {
            info!("sync_from_tip: already at tip (height={latest_u64})");
            self.current_height.store(latest_u64, Ordering::SeqCst);
            return Ok(format!("already at tip (height={latest_u64})"));
        }

        // Fetch blocks from from_height to latest
        info!("sync_from_tip: fetching blocks from {from_height:?} to {latest:?}");
        let blocks = lsp.get_block_range(from_height, latest).await?;
        let block_count = blocks.len();
        info!("sync_from_tip: fetched {block_count} blocks");

        if block_count == 0 {
            self.current_height.store(latest_u64, Ordering::SeqCst);
            return Ok("no new blocks".to_string());
        }

        let block_source = VecBlockSource {
            blocks: Arc::new(blocks),
        };

        // Scan from where we left off
        let scan_start = from_height;

        // Get tree state at from_height - 1
        let prev_height = if from_height > BlockHeight::from_u32(0) {
            from_height - 1
        } else {
            from_height
        };
        let tree_state = lsp.get_tree_state(prev_height).await?;
        let chain_state = tree_state
            .to_chain_state()
            .map_err(|e| format!("invalid tree state: {e}"))?;

        info!("sync_from_tip: scanning {block_count} blocks");
        let _scan_summary = scan_cached_blocks(
            &self.params,
            &block_source,
            &mut self.db,
            scan_start,
            &chain_state,
            block_count,
        )
        .map_err(|e| format!("scan_cached_blocks failed: {e}"))?;

        {
            info!("sync_from_tip: updating chain tip to {latest:?}");
            self.db
                .update_chain_tip(latest)
                .map_err(|e| format!("update_chain_tip failed: {e}"))?;
        }

        self.current_height.store(latest_u64, Ordering::SeqCst);
        info!("sync_from_tip: scan complete");

        let msg = format!("synced to block {}", latest_u64);
        Ok(msg)
    }
}

/// Try to find an account in the WalletDb by its 96-byte Orchard FVK.
fn lookup_account_by_fvk(
    db: &mut WalletDb<rusqlite::Connection, Network, SystemClock, OsRng>,
    fvk_bytes: &[u8],
) -> Result<Option<AccountUuid>, String> {
    let bytes: [u8; 96] = fvk_bytes
        .try_into()
        .map_err(|_| "FVK must be 96 bytes".to_string())?;

    let ufvk_item = zcash_address::unified::Fvk::Orchard(bytes);
    let ufvk_container = zcash_address::unified::Ufvk::try_from_items(vec![ufvk_item])
        .map_err(|e| format!("failed to build UFVK container: {e}"))?;
    let ufvk = UnifiedFullViewingKey::parse(&ufvk_container)
        .map_err(|e| format!("failed to parse UFVK: {e}"))?;

    match db.get_account_for_ufvk(&ufvk) {
        Ok(Some(acct)) => Ok(Some(acct.id())),
        Ok(None) => Ok(None),
        Err(e) => Err(format!("get_account_for_ufvk failed: {e}")),
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
                // Debug: log wallet summary before proposing
                match self.db.get_wallet_summary(self.confirmations_policy) {
                    Ok(Some(summary)) => {
                        for (aid, ab) in summary.account_balances() {
                            let ob = ab.orchard_balance();
                            info!(
                                "ProposeAndBuild: account={:?} orchard spendable={} pending_change={} pending_spendable={}",
                                aid,
                                u64::from(ob.spendable_value()),
                                u64::from(ob.change_pending_confirmation()),
                                u64::from(ob.value_pending_spendability()),
                            );
                        }
                    }
                    Ok(None) => info!("ProposeAndBuild: wallet summary is None (sync first?)"),
                    Err(e) => info!("ProposeAndBuild: get_wallet_summary error: {e}"),
                }

                let to_addr = zcash_address::ZcashAddress::try_from_encoded(&to)
                    .map_err(|e| format!("invalid recipient address: {e}"))?;

                let zcash_addr = to_addr
                    .convert()
                    .map_err(|e| format!("unsupported address type: {e}"))?;

                let amount_zat = zcash_protocol::value::Zatoshis::from_u64(amount)
                    .map_err(|_| "invalid amount".to_string())?;

                let account_ids = self
                    .db
                    .get_account_ids()
                    .map_err(|e| format!("get_account_ids failed: {e}"))?;

                // Find the first account with sufficient spendable balance
                let summary = self
                    .db
                    .get_wallet_summary(self.confirmations_policy)
                    .map_err(|e| format!("get_wallet_summary failed: {e}"))?
                    .ok_or("wallet summary not available")?;

                let account_id = account_ids
                    .iter()
                    .find(|aid| {
                        summary
                            .account_balances()
                            .get(aid)
                            .map(|b| u64::from(b.orchard_balance().spendable_value()) >= amount)
                            .unwrap_or(false)
                    })
                    .ok_or("no account with sufficient balance")?
                    .to_owned();

                info!(
                    "ProposeAndBuild: using account_id={:?} amount={} to={}",
                    account_id, amount, to,
                );

                let memo = memo
                    .as_deref()
                    .map(Memo::from_str)
                    .transpose()
                    .map_err(|e| format!("invalid memo: {e}"))?
                    .map(zcash_protocol::memo::MemoBytes::from);

                let build_params = self.build_params();
                let proposal = propose_standard_transfer_to_address::<
                    WalletDb<rusqlite::Connection, Network, SystemClock, OsRng>,
                    LocalNetwork,
                    SqliteClientError,
                >(
                    &mut self.db,
                    &build_params,
                    StandardFeeRule::Zip317,
                    account_id,
                    self.confirmations_policy,
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
                    LocalNetwork,
                    SqliteClientError,
                    StandardFeeRule,
                    SqliteClientError,
                    ReceivedNoteId,
                >(
                    &mut self.db,
                    &build_params,
                    account_id,
                    OvkPolicy::Sender,
                    &proposal,
                )
                .map_err(|e| format!("create_pczt_from_proposal failed: {e}"))?;

                Ok(pczt.serialize())
            }
            WalletMessage::RegisterAccount {
                fvk,
                birthday_height,
            } => {
                if self.is_syncing.load(Ordering::SeqCst) {
                    return Err("sync already in progress".to_string());
                }

                self.is_syncing.store(true, Ordering::SeqCst);
                self.current_height.store(0, Ordering::SeqCst);

                let sync_result = self.register_account(fvk, birthday_height).await;

                match &sync_result {
                    Ok(msg) => {
                        self.current_height
                            .store(self.target_height.load(Ordering::SeqCst), Ordering::SeqCst);
                        self.is_syncing.store(false, Ordering::SeqCst);
                        Ok(msg.clone().into_bytes())
                    }
                    Err(e) => {
                        self.is_syncing.store(false, Ordering::SeqCst);
                        Err(e.clone())
                    }
                }
            }
            WalletMessage::Sync => {
                if self.is_syncing.load(Ordering::SeqCst) {
                    return Err("sync already in progress".to_string());
                }

                if self.accounts.is_empty() {
                    return Ok("no accounts registered".to_string().into_bytes());
                }

                self.is_syncing.store(true, Ordering::SeqCst);

                let sync_result = self.sync_from_tip().await;

                match &sync_result {
                    Ok(msg) => {
                        self.is_syncing.store(false, Ordering::SeqCst);
                        Ok(msg.clone().into_bytes())
                    }
                    Err(e) => {
                        self.is_syncing.store(false, Ordering::SeqCst);
                        Err(e.clone())
                    }
                }
            }
            WalletMessage::GetStatus => {
                let status = SyncStatus {
                    is_syncing: self.is_syncing.load(Ordering::SeqCst),
                    current_height: self.current_height.load(Ordering::SeqCst),
                    target_height: self.target_height.load(Ordering::SeqCst),
                };
                postcard::to_allocvec(&status).map_err(|e| format!("serialize status failed: {e}"))
            }
            WalletMessage::GetBalance { viewing_key } => {
                info!("WalletMessage::GetBalance received by wallet actor");

                let target_uuid = self.fvk_to_account_id.get(&viewing_key).copied();

                let target_uuid = match target_uuid {
                    Some(uuid) => uuid,
                    None => {
                        // Not found in in-memory map — try to look up the account
                        // in the WalletDb directly. This handles the case where
                        // sync_account added the viewing key to the protocol's
                        // address_viewing_keys map but the sync itself failed
                        // (e.g. "sync already in progress"), so the wallet actor
                        // never registered it in fvk_to_account_id.
                        info!("WalletMessage::GetBalance: fvk not in memory map, querying WalletDb");
                        match lookup_account_by_fvk(&mut self.db, &viewing_key) {
                            Ok(Some(uuid)) => {
                                self.fvk_to_account_id.insert(viewing_key.clone(), uuid);
                                uuid
                            }
                            Ok(None) => {
                                info!("WalletMessage::GetBalance: viewing key not found in WalletDb, returning zero");
                                let balance = paypunk_types::Balance {
                                    spendable: Amount(0),
                                    pending: Amount(0),
                                    total: Amount(0),
                                };
                                return postcard::to_allocvec(&balance)
                                    .map_err(|e| format!("serialize balance failed: {e}"));
                            }
                            Err(e) => {
                                info!("WalletMessage::GetBalance: WalletDb lookup failed: {e}");
                                let balance = paypunk_types::Balance {
                                    spendable: Amount(0),
                                    pending: Amount(0),
                                    total: Amount(0),
                                };
                                return postcard::to_allocvec(&balance)
                                    .map_err(|e| format!("serialize balance failed: {e}"));
                            }
                        }
                    }
                };

                let summary = self
                    .db
                    .get_wallet_summary(self.confirmations_policy)
                    .map_err(|e| format!("get_wallet_summary failed: {e}"))?
                    .ok_or("wallet summary not available — sync first")?;

                let acct_balance = summary.account_balances().get(&target_uuid);

                let (spendable, pending, total) = match acct_balance {
                    Some(bal) => {
                        let ob = bal.orchard_balance();
                        let s: u64 = u64::from(ob.spendable_value());
                        let pc: u64 = u64::from(ob.change_pending_confirmation());
                        let ps: u64 = u64::from(ob.value_pending_spendability());
                        let pending = pc + ps;
                        (s as u128, pending as u128, (s + pending) as u128)
                    }
                    None => (0, 0, 0),
                };

                info!(
                    "WalletMessage::GetBalance: uuid={:?} spendable={}, pending={}, value={}",
                    target_uuid, spendable, pending, total
                );

                let balance = paypunk_types::Balance {
                    spendable: Amount(spendable),
                    pending: Amount(pending),
                    total: Amount(total),
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
            WalletMessage::StoreTransaction { pczt_bytes } => {
                let pczt = pczt::Pczt::parse(&pczt_bytes)
                    .map_err(|e| format!("PCZT parse failed: {e:?}"))?;
                let orchard_vk = orchard::circuit::VerifyingKey::build();
                let txid = extract_and_store_transaction_from_pczt::<
                    WalletDb<rusqlite::Connection, Network, SystemClock, OsRng>,
                    ReceivedNoteId,
                >(
                    &mut self.db,
                    pczt,
                    None,
                    Some(&orchard_vk),
                )
                .map_err(|e| format!("store transaction failed: {e}"))?;
                let txid_hex = hex::encode(txid.as_ref());
                Ok(txid_hex.into_bytes())
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
                let to_addr = zcash_address::ZcashAddress::try_from_encoded(&to)
                    .map_err(|e| format!("invalid recipient address: {e}"))?;

                let zcash_addr = to_addr
                    .convert()
                    .map_err(|e| format!("unsupported address type: {e}"))?;

                let amount_zat = zcash_protocol::value::Zatoshis::from_u64(amount)
                    .map_err(|_| "invalid amount".to_string())?;

                let account_ids = self
                    .db
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
                    &mut self.db,
                    &self.params,
                    StandardFeeRule::Zip317,
                    account_id,
                    self.confirmations_policy,
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
