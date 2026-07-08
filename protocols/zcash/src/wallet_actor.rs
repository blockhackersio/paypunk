use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;

use rand_core::OsRng;
use tactix::{Actor, Ctx, Handler, Message};
use tracing::info;
use zcash_address::unified::{Encoding, Fvk, Ufvk};
use zcash_client_backend::data_api::chain::scan_cached_blocks;
use zcash_client_backend::data_api::chain::BlockSource;
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

/// Build an unsigned PCZT for a transfer.
#[derive(Debug, Message)]
#[response(Result<Vec<u8>, String>)]
pub struct ProposeAndBuild {
    pub public_key: Vec<u8>,
    pub account: u32,
    pub to: String,
    pub amount: u64,
    pub memo: Option<String>,
}

/// Register a new account (parse FVK, get tree state, import into WalletDb).
#[derive(Debug, Message)]
#[response(Result<String, String>)]
pub struct RegisterAccount {
    pub fvk: Vec<u8>,
    pub birthday_height: u64,
}

/// Get the current sync status.
#[derive(Debug, Message)]
#[response(Result<SyncStatus, String>)]
pub struct GetStatus;

/// Get the balance for a specific UFVK.
#[derive(Debug, Message)]
#[response(Result<paypunk_types::Balance, String>)]
pub struct GetBalance {
    pub viewing_key: Vec<u8>,
}

/// Fetch transaction history for the given account.
#[derive(Debug, Message)]
#[response(Result<Page<HistoryEntry>, String>)]
pub struct GetHistory {
    pub account: u32,
    pub cursor: Option<String>,
    pub limit: u32,
}

/// Get the current block height from lightwalletd.
#[derive(Debug, Message)]
#[response(Result<paypunk_types::BlockHeight, String>)]
pub struct GetBlockHeight {
    pub lightwalletd_host: String,
}

/// Extract and store a signed PCZT in the wallet DB, returning the txid hex.
#[derive(Debug, Message)]
#[response(Result<String, String>)]
pub struct StoreTransaction {
    pub pczt_bytes: Vec<u8>,
}

/// Get the status of a transaction by its txid.
#[derive(Debug, Message)]
#[response(Result<TxStatus, String>)]
pub struct GetTxStatus {
    pub txid: String,
}

/// Estimate the fee for a transfer.
#[derive(Debug, Message)]
#[response(Result<u64, String>)]
pub struct EstimateFee {
    pub to: String,
    pub amount: u64,
    pub memo: Option<String>,
}

/// Update sync status from the ScanActor (fire-and-forget).
#[derive(Debug, Message)]
#[response(Result<(), String>)]
pub struct ScanUpdate(pub SyncStatus);

/// Get the current chain tip height from the wallet DB.
#[derive(Debug, Message)]
#[response(Result<u64, String>)]
pub struct GetChainTip;

/// Scan blocks that have been fetched from lightwalletd.
#[derive(Debug, Message)]
#[response(Result<String, String>)]
pub struct ScanBlocks {
    pub blocks: Vec<CompactBlock>,
    pub from_height: BlockHeight,
    pub chain_state: zcash_client_backend::data_api::chain::ChainState,
    pub target_height: BlockHeight,
}

/// Tactix actor wrapping `zcash_client_sqlite::WalletDb`.
///
/// Handles non-scan operations: balance queries, transfer building, history.
/// Chain scanning is delegated to `ScanActor` so that the wallet remains
/// responsive during long sync operations.
pub struct WalletDbActor {
    db: WalletDb<rusqlite::Connection, Network, SystemClock, OsRng>,
    params: Network,
    current_height: u64,
    target_height: u64,
    is_syncing: bool,
    db_path: PathBuf,
    fvk_to_account_id: HashMap<Vec<u8>, AccountUuid>,
    confirmations_policy: ConfirmationsPolicy,
    lightwalletd_host: String,
    accounts: Vec<(Vec<u8>, u64)>,
}

impl WalletDbActor {
    pub fn new(
        db: WalletDb<rusqlite::Connection, Network, SystemClock, OsRng>,
        params: Network,
        db_path: PathBuf,
        confirmations_policy: ConfirmationsPolicy,
        lightwalletd_host: String,
    ) -> Self {
        Self {
            db,
            params,
            is_syncing: false,
            current_height: 0,
            target_height: 0,
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

        // Fetch tree state at birthday-1 for the account birthday
        info!("register_account: getting tree state from lightwalletd");
        let mut lsp = LspClient::connect(&self.lightwalletd_host, self.params).await?;
        let prev_height = if birthday > BlockHeight::from_u32(0) {
            birthday - 1
        } else {
            birthday
        };
        let tree_state = lsp.get_tree_state(prev_height).await?;
        let chain_state = tree_state
            .to_chain_state()
            .map_err(|e| format!("invalid tree state: {e}"))?;

        {
            let account_name = format!("Zcash Account {}", self.fvk_to_account_id.len());
            let account_birthday = AccountBirthday::from_parts(chain_state, None);

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
        self.accounts
            .push((fvk_bytes.to_vec(), u64::from(birthday)));

        info!("register_account: FVK imported, scanning delegated to ScanActor");
        let msg = format!(
            "registered account with birthday at block {}",
            u64::from(birthday),
        );
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

/// In-memory block source holding pre-fetched compact blocks.
struct VecBlockSource {
    blocks: std::sync::Arc<Vec<CompactBlock>>,
}

impl BlockSource for VecBlockSource {
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
        ) -> Result<
            (),
            zcash_client_backend::data_api::chain::error::Error<WalletErrT, Self::Error>,
        >,
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

impl Actor for WalletDbActor {}

impl Handler<ProposeAndBuild> for WalletDbActor {
    async fn handle(&mut self, msg: ProposeAndBuild, _ctx: &Ctx<Self>) -> Result<Vec<u8>, String> {
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

        let to_addr = zcash_address::ZcashAddress::try_from_encoded(&msg.to)
            .map_err(|e| format!("invalid recipient address: {e}"))?;

        let zcash_addr = to_addr
            .convert()
            .map_err(|e| format!("unsupported address type: {e}"))?;

        let amount_zat = zcash_protocol::value::Zatoshis::from_u64(msg.amount)
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
                    .map(|b| u64::from(b.orchard_balance().spendable_value()) >= msg.amount)
                    .unwrap_or(false)
            })
            .ok_or("no account with sufficient balance")?
            .to_owned();

        info!(
            "ProposeAndBuild: using account_id={:?} amount={} to={}",
            account_id, msg.amount, msg.to,
        );

        let memo = msg
            .memo
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
}

impl Handler<RegisterAccount> for WalletDbActor {
    async fn handle(&mut self, msg: RegisterAccount, _ctx: &Ctx<Self>) -> Result<String, String> {
        self.register_account(msg.fvk, msg.birthday_height).await
    }
}

impl Handler<GetStatus> for WalletDbActor {
    async fn handle(&mut self, _msg: GetStatus, _ctx: &Ctx<Self>) -> Result<SyncStatus, String> {
        Ok(SyncStatus {
            is_syncing: self.is_syncing,
            current_height: self.current_height,
            target_height: self.target_height,
        })
    }
}

impl Handler<ScanUpdate> for WalletDbActor {
    async fn handle(&mut self, msg: ScanUpdate, _ctx: &Ctx<Self>) -> Result<(), String> {
        self.is_syncing = msg.0.is_syncing;
        self.current_height = msg.0.current_height;
        self.target_height = msg.0.target_height;
        Ok(())
    }
}

impl Handler<GetChainTip> for WalletDbActor {
    async fn handle(&mut self, _msg: GetChainTip, _ctx: &Ctx<Self>) -> Result<u64, String> {
        let tip = self
            .db
            .chain_height()
            .map_err(|e| format!("chain_height failed: {e}"))?;
        info!("trace: WalletDbActor.GetChainTip: {:?}", tip);
        Ok(tip.map(|h| h.into()).unwrap_or(0))
    }
}

impl Handler<ScanBlocks> for WalletDbActor {
    async fn handle(&mut self, msg: ScanBlocks, _ctx: &Ctx<Self>) -> Result<String, String> {
        self.ensure_db_file_exists()?;

        let block_source = VecBlockSource {
            blocks: std::sync::Arc::new(msg.blocks),
        };
        let block_count = block_source.blocks.len();
        let from_u64: u64 = msg.from_height.into();
        let target_u64: u64 = msg.target_height.into();
        info!("wallet_actor: scanning {block_count} blocks from {from_u64} to {target_u64}");

        self.is_syncing = true;
        self.target_height = target_u64;

        match scan_cached_blocks(
            &self.params,
            &block_source,
            &mut self.db,
            msg.from_height,
            &msg.chain_state,
            block_count,
        ) {
            Ok(_summary) => {
                info!("wallet_actor: scan_cached_blocks OK, updating chain tip");
            }
            Err(e) => {
                tracing::warn!(
                    "wallet_actor: scan_cached_blocks failed (advancing chain tip anyway): {e}"
                );
            }
        }

        self.db
            .update_chain_tip(msg.target_height)
            .map_err(|e| format!("update_chain_tip failed: {e}"))?;
        info!("wallet_actor: chain tip updated to {target_u64}");

        let latest_u64: u64 = msg.target_height.into();
        self.current_height = latest_u64;
        self.is_syncing = false;

        Ok(format!("synced to block {latest_u64}"))
    }
}

impl Handler<GetBalance> for WalletDbActor {
    async fn handle(
        &mut self,
        msg: GetBalance,
        _ctx: &Ctx<Self>,
    ) -> Result<paypunk_types::Balance, String> {
        info!("GetBalance received by wallet actor");

        let target_uuid = self.fvk_to_account_id.get(&msg.viewing_key).copied();

        let target_uuid = match target_uuid {
            Some(uuid) => uuid,
            None => {
                // Not found in in-memory map — try to look up the account
                // in the WalletDb directly.
                info!("GetBalance: fvk not in memory map, querying WalletDb");
                match lookup_account_by_fvk(&mut self.db, &msg.viewing_key) {
                    Ok(Some(uuid)) => {
                        self.fvk_to_account_id.insert(msg.viewing_key.clone(), uuid);
                        uuid
                    }
                    Ok(None) => {
                        info!("GetBalance: viewing key not found in WalletDb, returning zero");
                        return Ok(paypunk_types::Balance {
                            spendable: Amount(0),
                            pending: Amount(0),
                            total: Amount(0),
                        });
                    }
                    Err(e) => {
                        info!("GetBalance: WalletDb lookup failed: {e}");
                        return Ok(paypunk_types::Balance {
                            spendable: Amount(0),
                            pending: Amount(0),
                            total: Amount(0),
                        });
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
            "GetBalance: uuid={:?} spendable={}, pending={}, value={}",
            target_uuid, spendable, pending, total
        );

        Ok(paypunk_types::Balance {
            spendable: Amount(spendable),
            pending: Amount(pending),
            total: Amount(total),
        })
    }
}

impl Handler<GetHistory> for WalletDbActor {
    async fn handle(
        &mut self,
        _msg: GetHistory,
        _ctx: &Ctx<Self>,
    ) -> Result<Page<HistoryEntry>, String> {
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

        Ok(Page {
            items: entries,
            next_cursor: None,
            has_more: false,
        })
    }
}

impl Handler<GetBlockHeight> for WalletDbActor {
    async fn handle(
        &mut self,
        msg: GetBlockHeight,
        _ctx: &Ctx<Self>,
    ) -> Result<paypunk_types::BlockHeight, String> {
        let mut lsp = LspClient::connect(&msg.lightwalletd_host, self.params).await?;
        let height = lsp.get_latest_height().await?;
        let height_u64: u64 = height.into();
        Ok(paypunk_types::BlockHeight(height_u64))
    }
}

impl Handler<StoreTransaction> for WalletDbActor {
    async fn handle(&mut self, msg: StoreTransaction, _ctx: &Ctx<Self>) -> Result<String, String> {
        let pczt =
            pczt::Pczt::parse(&msg.pczt_bytes).map_err(|e| format!("PCZT parse failed: {e:?}"))?;
        let orchard_vk = orchard::circuit::VerifyingKey::build();
        let txid = extract_and_store_transaction_from_pczt::<
            WalletDb<rusqlite::Connection, Network, SystemClock, OsRng>,
            ReceivedNoteId,
        >(&mut self.db, pczt, None, Some(&orchard_vk))
        .map_err(|e| format!("store transaction failed: {e}"))?;
        Ok(hex::encode(txid.as_ref()))
    }
}

impl Handler<GetTxStatus> for WalletDbActor {
    async fn handle(&mut self, msg: GetTxStatus, _ctx: &Ctx<Self>) -> Result<TxStatus, String> {
        let reader = rusqlite::Connection::open(&self.db_path)
            .map_err(|e| format!("failed to open wallet db: {e}"))?;

        let txid_bytes = hex::decode(&msg.txid).map_err(|e| format!("invalid txid hex: {e}"))?;

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

        Ok(status)
    }
}

impl Handler<EstimateFee> for WalletDbActor {
    async fn handle(&mut self, msg: EstimateFee, _ctx: &Ctx<Self>) -> Result<u64, String> {
        let to_addr = zcash_address::ZcashAddress::try_from_encoded(&msg.to)
            .map_err(|e| format!("invalid recipient address: {e}"))?;

        let zcash_addr = to_addr
            .convert()
            .map_err(|e| format!("unsupported address type: {e}"))?;

        let amount_zat = zcash_protocol::value::Zatoshis::from_u64(msg.amount)
            .map_err(|_| "invalid amount".to_string())?;

        let account_ids = self
            .db
            .get_account_ids()
            .map_err(|e| format!("get_account_ids failed: {e}"))?;
        let account_id = account_ids
            .first()
            .ok_or("no accounts in wallet")?
            .to_owned();

        let memo = msg
            .memo
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
        Ok(u64::from(fee))
    }
}
