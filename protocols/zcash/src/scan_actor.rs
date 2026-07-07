use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use rand_core::OsRng;
use tactix::{Actor, Ctx, Handler, Message, Recipient, Sender};
use tokio;
use tracing::info;
use zcash_client_backend::data_api::chain::scan_cached_blocks;
use zcash_client_backend::data_api::WalletWrite;
use zcash_client_sqlite::util::SystemClock;
use zcash_client_sqlite::WalletDb;
use zcash_protocol::consensus::{BlockHeight, Network};

use crate::lsp_client::LspClient;
use crate::wallet_actor::WalletMessage;

/// Messages sent to the ScanActor.
#[derive(Debug, Message)]
#[response(Result<Vec<u8>, String>)]
pub enum ScanMessage {
    /// Incremental sync from current chain tip using stored accounts.
    Sync,
    /// Initial sync for a newly registered account from its birthday.
    SyncNewAccount { birthday_height: u64 },
}

/// In-memory block source holding pre-fetched compact blocks.
struct VecBlockSource {
    blocks: Arc<Vec<zcash_client_backend::proto::compact_formats::CompactBlock>>,
}

impl zcash_client_backend::data_api::chain::BlockSource for VecBlockSource {
    type Error = String;

    fn with_blocks<F, WalletErrT>(
        &self,
        from_height: Option<BlockHeight>,
        limit: Option<usize>,
        mut with_block: F,
    ) -> Result<(), zcash_client_backend::data_api::chain::error::Error<WalletErrT, Self::Error>>
    where
        F: FnMut(
            zcash_client_backend::proto::compact_formats::CompactBlock,
        ) -> Result<(), zcash_client_backend::data_api::chain::error::Error<WalletErrT, Self::Error>>,
    {
        let from = from_height.map(u64::from).unwrap_or(0);
        let limit = limit.unwrap_or(usize::MAX);
        let mut count = 0;
        for block in self.blocks.iter() {
            let h = block.height;
            if h >= from && count < limit {
                with_block(block.clone()).map_err(|e| match e {
                    zcash_client_backend::data_api::chain::error::Error::Wallet(e) => {
                        zcash_client_backend::data_api::chain::error::Error::Wallet(e)
                    }
                    zcash_client_backend::data_api::chain::error::Error::BlockSource(e) => {
                        zcash_client_backend::data_api::chain::error::Error::BlockSource(e)
                    }
                    zcash_client_backend::data_api::chain::error::Error::Scan(e) => {
                        zcash_client_backend::data_api::chain::error::Error::Scan(e)
                    }
                })?;
                count += 1;
            }
        }
        Ok(())
    }
}

/// Tactix actor that handles chain scanning independently of the WalletDbActor.
///
/// Owns a separate `WalletDb` connection to the same SQLite database so that
/// scanning (which is I/O-heavy and takes many seconds) does not block
/// balance queries, transfer building, or other wallet interactions.
pub struct ScanActor {
    db: WalletDb<rusqlite::Connection, Network, SystemClock, OsRng>,
    params: Network,
    db_path: PathBuf,
    lightwalletd_host: String,
    wallet_actor: Recipient<WalletMessage>,
    is_syncing: AtomicBool,
    current_height: AtomicU64,
    target_height: AtomicU64,
}

impl ScanActor {
    pub fn new(
        db: WalletDb<rusqlite::Connection, Network, SystemClock, OsRng>,
        params: Network,
        db_path: PathBuf,
        lightwalletd_host: String,
        wallet_actor: Recipient<WalletMessage>,
    ) -> Self {
        Self {
            db,
            params,
            db_path,
            lightwalletd_host,
            wallet_actor,
            is_syncing: AtomicBool::new(false),
            current_height: AtomicU64::new(0),
            target_height: AtomicU64::new(0),
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

    async fn sync_from_tip(&mut self) -> Result<String, String> {
        self.ensure_db_file_exists()?;

        info!("scan_actor: connecting to lightwalletd at {}", self.lightwalletd_host);
        let mut lsp = LspClient::connect(&self.lightwalletd_host, self.params).await?;
        let latest = lsp.get_latest_height().await?;
        let latest_u64: u64 = latest.into();
        self.target_height.store(latest_u64, Ordering::SeqCst);
        info!("scan_actor: latest height = {latest_u64}");

        // Get current chain tip from wallet DB
        let chain_tip = self
            .db
            .chain_height()
            .map_err(|e| format!("chain_height failed: {e}"))?;

        let from_height = match chain_tip {
            Some(tip) => {
                let next = tip + 1;
                info!("scan_actor: current chain tip is {tip:?}, fetching from {next:?}");
                next
            }
            None => {
                info!("scan_actor: no chain tip, nothing to sync");
                self.current_height.store(latest_u64, Ordering::SeqCst);
                self.notify_wallet();
                return Ok("no chain tip, nothing to sync".to_string());
            }
        };

        if from_height > latest {
            info!("scan_actor: already at tip (height={latest_u64})");
            self.current_height.store(latest_u64, Ordering::SeqCst);
            self.notify_wallet();
            return Ok(format!("already at tip (height={latest_u64})"));
        }

        // Fetch blocks from from_height to latest
        info!("scan_actor: fetching blocks from {from_height:?} to {latest:?}");
        let blocks = lsp.get_block_range(from_height, latest).await?;
        let block_count = blocks.len();
        info!("scan_actor: fetched {block_count} blocks");

        if block_count == 0 {
            self.current_height.store(latest_u64, Ordering::SeqCst);
            self.notify_wallet();
            return Ok("no new blocks".to_string());
        }

        let block_source = VecBlockSource {
            blocks: Arc::new(blocks),
        };

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

        info!("scan_actor: scanning {block_count} blocks");
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
            info!("scan_actor: updating chain tip to {latest:?}");
            self.db
                .update_chain_tip(latest)
                .map_err(|e| format!("update_chain_tip failed: {e}"))?;
        }

        self.current_height.store(latest_u64, Ordering::SeqCst);
        self.notify_wallet();
        info!("scan_actor: scan complete");

        let msg = format!("synced to block {}", latest_u64);
        Ok(msg)
    }

    async fn sync_new_account(&mut self, birthday_height: u64) -> Result<String, String> {
        self.ensure_db_file_exists()?;

        info!("scan_actor: initial sync for new account from birthday={birthday_height}");
        let mut lsp = LspClient::connect(&self.lightwalletd_host, self.params).await?;
        let latest = lsp.get_latest_height().await?;
        let latest_u64: u64 = latest.into();
        self.target_height.store(latest_u64, Ordering::SeqCst);

        let birthday = if birthday_height == 0 {
            BlockHeight::from_u32(2)
        } else {
            BlockHeight::from_u32(birthday_height as u32)
        };

        if birthday > latest {
            info!("scan_actor: birthday after latest tip, nothing to sync");
            self.current_height.store(latest_u64, Ordering::SeqCst);
            self.notify_wallet();
            return Ok("birthday after tip, nothing to sync".to_string());
        }

        info!("scan_actor: fetching blocks from {birthday:?} to {latest:?}");
        let blocks = lsp.get_block_range(birthday, latest).await?;
        let block_count = blocks.len();
        info!("scan_actor: fetched {block_count} blocks");

        let block_source = VecBlockSource {
            blocks: Arc::new(blocks),
        };

        let prev_height = if birthday > BlockHeight::from_u32(0) {
            birthday - 1
        } else {
            birthday
        };
        let tree_state = lsp.get_tree_state(prev_height).await?;
        let chain_state = tree_state
            .to_chain_state()
            .map_err(|e| format!("invalid tree state: {e}"))?;

        info!("scan_actor: scanning {block_count} blocks");
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
            info!("scan_actor: updating chain tip to {latest:?}");
            self.db
                .update_chain_tip(latest)
                .map_err(|e| format!("update_chain_tip failed: {e}"))?;
        }

        self.current_height.store(latest_u64, Ordering::SeqCst);
        self.notify_wallet();
        info!("scan_actor: initial sync complete");

        let msg = format!(
            "synced new account from block {} to {}",
            u64::from(birthday),
            latest_u64
        );
        Ok(msg)
    }

    /// Send a status update to the WalletDbActor so its `GetStatus` responses
    /// reflect the latest scan progress.
    fn notify_wallet(&self) {
        let status = paypunk_types::SyncStatus {
            is_syncing: self.is_syncing.load(Ordering::SeqCst),
            current_height: self.current_height.load(Ordering::SeqCst),
            target_height: self.target_height.load(Ordering::SeqCst),
        };
        let recipient = self.wallet_actor.clone();
        tokio::spawn(async move {
            let _ = recipient.ask(WalletMessage::ScanUpdate(status)).await;
        });
    }
}

impl Actor for ScanActor {}

impl Handler<ScanMessage> for ScanActor {
    async fn handle(
        &mut self,
        msg: ScanMessage,
        _ctx: &Ctx<Self>,
    ) -> Result<Vec<u8>, String> {
        match msg {
            ScanMessage::Sync => {
                if self.is_syncing.load(Ordering::SeqCst) {
                    return Err("scan already in progress".to_string());
                }
                self.is_syncing.store(true, Ordering::SeqCst);

                let result = self.sync_from_tip().await;

                match &result {
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
            ScanMessage::SyncNewAccount { birthday_height } => {
                if self.is_syncing.load(Ordering::SeqCst) {
                    return Err("scan already in progress".to_string());
                }
                self.is_syncing.store(true, Ordering::SeqCst);

                let result = self.sync_new_account(birthday_height).await;

                match &result {
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
        }
    }
}
