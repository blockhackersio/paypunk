//! Reference example: full shielded Zcash wallet flow against lightwalletd.
//!
//! Demonstrates: seed generation, key derivation, SQLite wallet init, chain sync,
//! balance check, shielded transfer proposal + creation, tx submission, re-sync.
//!
//! Run: cargo run --example send_shielded -- <LIGHTWALLETD_URL> <DEST_ADDRESS>
//! Example: cargo run --example send_shielded -- https://mainnet.lightwalletd.com:9067 u1...

use std::num::NonZeroU32;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Mutex;

use bip39::{Language, Mnemonic};
use prost::Message;
use rand_core::OsRng;
use rusqlite::Connection;
use secrecy::SecretVec;
use tonic::transport::Channel;
use zcash_address::ZcashAddress;
use zcash_client_backend::address::Address;
use zcash_client_backend::data_api::chain::{
    scan_cached_blocks, BlockCache, BlockSource, ChainState,
};
use zcash_client_backend::data_api::scanning::ScanRange;
use zcash_client_backend::data_api::wallet::{
    create_proposed_transactions, propose_standard_transfer_to_address, SpendingKeys,
};
use zcash_client_backend::data_api::{
    wallet::ConfirmationsPolicy, AccountBirthday, WalletRead, WalletWrite,
};
use zcash_client_backend::fees::StandardFeeRule;
use zcash_client_backend::keys::UnifiedSpendingKey;
use zcash_client_backend::proto::compact_formats::CompactBlock;
use zcash_client_backend::proto::service::compact_tx_streamer_client::CompactTxStreamerClient;
use zcash_client_backend::proto::service::BlockId;
use zcash_client_backend::wallet::OvkPolicy;
use zcash_client_sqlite::error::SqliteClientError;
use zcash_client_sqlite::util::SystemClock;
use zcash_client_sqlite::WalletDb;
use zcash_primitives::block::BlockHash;
use zcash_protocol::consensus::BlockHeight;
use zcash_protocol::value::Zatoshis;
use zip32::AccountId;

// ── Custom block cache ─────────────────────────────────────────────────
//
// We need a local type to implement BlockCache (orphan rule).

struct CacheDb {
    conn: Mutex<Connection>,
}

impl CacheDb {
    fn new() -> Self {
        let conn = Connection::open_in_memory().expect("in-memory cache db");
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS compactblocks (
                height INTEGER PRIMARY KEY,
                data BLOB NOT NULL
            ); PRAGMA journal_mode=wal; PRAGMA synchronous=NORMAL;",
        )
        .ok();
        CacheDb {
            conn: Mutex::new(conn),
        }
    }

    fn insert_blocks(&self, blocks: &[CompactBlock]) {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().expect("cache tx");
        for cb in blocks {
            let data = cb.encode_to_vec();
            tx.execute(
                "INSERT OR REPLACE INTO compactblocks (height, data) VALUES (?1, ?2)",
                rusqlite::params![u32::from(cb.height()), data],
            )
            .expect("insert block");
        }
        tx.commit().expect("commit cache");
    }
}

impl BlockSource for CacheDb {
    type Error = SqliteClientError;

    fn with_blocks<F, DbErrT>(
        &self,
        from_height: Option<BlockHeight>,
        limit: Option<usize>,
        mut with_row: F,
    ) -> Result<(), zcash_client_backend::data_api::chain::error::Error<DbErrT, Self::Error>>
    where
        F: FnMut(CompactBlock) -> Result<(), zcash_client_backend::data_api::chain::error::Error<DbErrT, Self::Error>>,
    {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT height, data FROM compactblocks
                 WHERE height >= ?
                 ORDER BY height ASC LIMIT ?",
            )
            .map_err(|e| zcash_client_backend::data_api::chain::error::Error::BlockSource(e.into()))?;

        let mut rows = stmt
            .query(rusqlite::params![
                from_height.map_or(0u32, u32::from),
                limit.and_then(|l| u32::try_from(l).ok()).unwrap_or(u32::MAX)
            ])
            .map_err(|e| zcash_client_backend::data_api::chain::error::Error::BlockSource(e.into()))?;

        let mut from_height_found = from_height.is_none();
        while let Some(row) = rows
            .next()
            .map_err(|e| zcash_client_backend::data_api::chain::error::Error::BlockSource(e.into()))?
        {
            let height: u32 = row.get(0).map_err(|e| {
                zcash_client_backend::data_api::chain::error::Error::BlockSource(e.into())
            })?;
            let height = BlockHeight::from_u32(height);
            if !from_height_found {
                let fh = from_height.expect("can only reach here if set");
                if fh != height {
                    return Err(zcash_client_backend::data_api::chain::error::Error::BlockSource(
                        SqliteClientError::CacheMiss(fh),
                    ));
                }
                from_height_found = true;
            }

            let data: Vec<u8> = row.get(1).map_err(|e| {
                zcash_client_backend::data_api::chain::error::Error::BlockSource(e.into())
            })?;
            let block = CompactBlock::decode(&data[..]).map_err(|e| {
                zcash_client_backend::data_api::chain::error::Error::BlockSource(
                    SqliteClientError::CorruptedData(e.to_string()),
                )
            })?;
            if block.height() != height {
                return Err(zcash_client_backend::data_api::chain::error::Error::BlockSource(
                    SqliteClientError::CorruptedData(format!(
                        "compact block at height {} has mismatched height field {}",
                        u32::from(height),
                        u32::from(block.height()),
                    )),
                ));
            }
            with_row(block)?;
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl BlockCache for CacheDb {
    fn get_tip_height(
        &self,
        _range: Option<&ScanRange>,
    ) -> Result<Option<BlockHeight>, Self::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT MAX(height) FROM compactblocks")
            .map_err(SqliteClientError::from)?;
        let tip: Option<u32> = stmt
            .query_row([], |row| row.get(0))
            .map_err(SqliteClientError::from)?;
        Ok(tip.map(BlockHeight::from_u32))
    }

    async fn read(&self, range: &ScanRange) -> Result<Vec<CompactBlock>, Self::Error> {
        let block_range = range.block_range();
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT height, data FROM compactblocks
                 WHERE height >= ? AND height < ?
                 ORDER BY height ASC",
            )
            .map_err(SqliteClientError::from)?;
        let rows = stmt
            .query_map(
                rusqlite::params![u32::from(block_range.start), u32::from(block_range.end)],
                |row| {
                    let data: Vec<u8> = row.get(1)?;
                    Ok(data)
                },
            )
            .map_err(SqliteClientError::from)?;
        let mut blocks = Vec::new();
        for row in rows {
            let data = row.map_err(SqliteClientError::from)?;
            blocks.push(
                CompactBlock::decode(&data[..])
                    .map_err(|e| SqliteClientError::CorruptedData(e.to_string()))?,
            );
        }
        Ok(blocks)
    }

    async fn insert(&self, compact_blocks: Vec<CompactBlock>) -> Result<(), Self::Error> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().map_err(SqliteClientError::from)?;
        for cb in &compact_blocks {
            let data = cb.encode_to_vec();
            tx.execute(
                "INSERT OR REPLACE INTO compactblocks (height, data) VALUES (?1, ?2)",
                rusqlite::params![u32::from(cb.height()), data],
            )
            .map_err(SqliteClientError::from)?;
        }
        tx.commit().map_err(SqliteClientError::from)?;
        Ok(())
    }

    async fn delete(&self, range: ScanRange) -> Result<(), Self::Error> {
        let block_range = range.block_range();
        let conn = self.conn.lock().unwrap();
        conn.execute(
                "DELETE FROM compactblocks WHERE height >= ? AND height < ?",
                rusqlite::params![u32::from(block_range.start), u32::from(block_range.end)],
            )
            .map_err(SqliteClientError::from)?;
        Ok(())
    }
}

// ── Helpers ────────────────────────────────────────────────────────────

fn generate_seed() -> ([u8; 64], String) {
    let mnemonic = Mnemonic::generate_in(Language::English, 12).expect("generate mnemonic");
    let seed = mnemonic.to_seed_normalized("");
    let phrase = mnemonic.to_string();
    let mut seed_bytes = [0u8; 64];
    seed_bytes.copy_from_slice(&seed[..64]);
    (seed_bytes, phrase)
}

async fn download_blocks(
    client: &mut CompactTxStreamerClient<tonic::transport::Channel>,
    start_height: BlockHeight,
    end_height: BlockHeight,
) -> Vec<CompactBlock> {
    use futures_util::TryStreamExt;
    use tonic::Request;

    let mut blocks = Vec::new();
    let mut height = start_height;
    while height < end_height {
        let batch_end = BlockHeight::from_u32(
            std::cmp::min(u32::from(height) + 1000, u32::from(end_height)),
        );
        let req = zcash_client_backend::proto::service::BlockRange {
            start: Some(BlockId {
                height: u64::from(u32::from(height)),
                hash: vec![],
            }),
            end: Some(BlockId {
                height: u64::from(u32::from(batch_end)),
                hash: vec![],
            }),
            pool_types: vec![],
        };
        let mut stream = client
            .get_block_range(Request::new(req))
            .await
            .expect("get_block_range")
            .into_inner();
        while let Some(block) = stream.try_next().await.expect("block stream") {
            blocks.push(block);
        }
        height = batch_end;
    }
    blocks
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    // ── CLI args ──────────────────────────────────────────────────────────
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <LIGHTWALLETD_URL> <DEST_ADDRESS>", args[0]);
        std::process::exit(1);
    }
    let lightwalletd_url = &args[1];
    let dest_addr_str = &args[2];

    // ── Network ───────────────────────────────────────────────────────────
    let params = zcash_protocol::consensus::TEST_NETWORK;

    // ── Step 1: Generate seed & derive keys ───────────────────────────────
    println!("=== Step 1: Generate seed & derive keys ===");
    let (seed, phrase) = generate_seed();
    println!("Mnemonic: {phrase}");

    let usk =
        UnifiedSpendingKey::from_seed(&params, &seed[..], AccountId::ZERO).expect("derive USK");
    println!("Key derived");

    // ── Step 2: Initialize SQLite databases ───────────────────────────────
    println!("\n=== Step 2: Initialize SQLite databases ===");
    let cache_db = CacheDb::new();
    let data_dir = PathBuf::from("/tmp/paypunk-reference");
    std::fs::create_dir_all(&data_dir).ok();
    let db_path = data_dir.join("wallet.db");

    let mut wallet_db = WalletDb::for_path(&db_path, params, SystemClock, OsRng)
        .expect("create wallet db");
    println!("Wallet DB: {:?}", db_path);

    // ── Step 3: Create account ────────────────────────────────────────────
    println!("\n=== Step 3: Create account ===");
    let birthday = AccountBirthday::from_parts(
        ChainState::empty(BlockHeight::from_u32(2800000), BlockHash([0u8; 32])),
        None,
    );
    let secret_seed = SecretVec::new(seed.to_vec());
    let (account_id, _) = wallet_db
        .create_account("default", &secret_seed, &birthday, None)
        .expect("create account");
    println!("Account ID: {account_id:?}");

    // ── Step 4: Connect to lightwalletd & sync ────────────────────────────
    println!("\n=== Step 4: Connect to lightwalletd & sync ===");
    let channel = Channel::from_shared(lightwalletd_url.to_string())
        .expect("valid channel uri")
        .connect()
        .await
        .expect("connect to lightwalletd");
    let mut client = CompactTxStreamerClient::new(channel);
    println!("Connected to {lightwalletd_url}");

    // Get chain tip from lightwalletd
    use tonic::Request;
    let tip = client
        .get_latest_block(zcash_client_backend::proto::service::ChainSpec {})
        .await
        .expect("get latest block")
        .into_inner();
    let tip_height = BlockHeight::from_u32(tip.height as u32);
    println!("Chain tip height: {tip_height}");

    // Download compact blocks from birthday to tip
    let birthday_height = BlockHeight::from_u32(2800000);
    println!("Downloading blocks from {birthday_height} to {tip_height}...");
    let blocks = download_blocks(&mut client, birthday_height, tip_height + 1).await;
    println!("Downloaded {} blocks", blocks.len());

    // Insert blocks into cache
    cache_db.insert_blocks(&blocks);

    // Scan cached blocks into the wallet
    let from_state = ChainState::empty(birthday_height - 1, BlockHash([0u8; 32]));
    let summary = scan_cached_blocks(
        &params,
        &cache_db,
        &mut wallet_db,
        birthday_height,
        &from_state,
        usize::MAX,
    )
    .expect("scan blocks");
    println!(
        "Scanned {} blocks, received {} sapling notes, {} orchard notes",
        u32::from(summary.scanned_range().end) - u32::from(summary.scanned_range().start),
        summary.received_sapling_note_count(),
        summary.received_orchard_note_count(),
    );

    // ── Step 5: Check balance ─────────────────────────────────────────────
    println!("\n=== Step 5: Check balance ===");
    let wallet_summary = wallet_db
        .get_wallet_summary(Default::default())
        .expect("get wallet summary")
        .expect("wallet is synced");
    let balance = wallet_summary
        .account_balances()
        .get(&account_id)
        .expect("account balance");
    println!(
        "Balance: orchard={} zat, sapling={} zat",
        balance.orchard_balance().total().into_u64(),
        balance.sapling_balance().total().into_u64(),
    );

    if balance.orchard_balance().total() == Zatoshis::ZERO
        && balance.sapling_balance().total() == Zatoshis::ZERO
    {
        println!("No spendable funds. Send some testnet ZEC to an address derived from this seed and re-run.");
        println!("Exiting without sending a transfer.");
        return Ok(());
    }

    // ── Step 6: Propose & create a shielded transfer ──────────────────────
    println!("\n=== Step 6: Send shielded transfer ===");
    let amount = Zatoshis::from_u64(100_000).expect("100_000 zat"); // 0.001 ZEC
    let dest_zaddr = ZcashAddress::from_str(dest_addr_str).expect("parse dest address");
    let dest_addr: Address = dest_zaddr.convert().expect("convert to Address");

    let confirmations =
        ConfirmationsPolicy::new(NonZeroU32::MIN, NonZeroU32::MIN, true).expect("min confirmations");

    use zcash_client_sqlite::wallet::commitment_tree;

    let proposal = propose_standard_transfer_to_address::<
        _,
        _,
        commitment_tree::Error,
    >(
        &mut wallet_db,
        &params,
        StandardFeeRule::Zip317,
        account_id,
        confirmations,
        &dest_addr,
        amount,
        None,
        None,
        zcash_protocol::ShieldedProtocol::Orchard,
    )
    .expect("propose transfer");
    println!("Proposal created: {} transactions", proposal.steps().len());

    let spending_keys = SpendingKeys::from_unified_spending_key(usk);
    use std::convert::Infallible;
    use zcash_client_sqlite::ReceivedNoteId;

    let tx_ids = create_proposed_transactions::<
        _,
        _,
        zcash_client_sqlite::error::SqliteClientError,
        StandardFeeRule,
        Infallible,
        ReceivedNoteId,
    >(
        &mut wallet_db,
        &params,
        &zcash_proofs::prover::LocalTxProver::bundled(),
        &zcash_proofs::prover::LocalTxProver::bundled(),
        &spending_keys,
        OvkPolicy::Sender,
        &proposal,
    )
    .expect("create proposed transactions");
    println!("Transaction created: {:?}", tx_ids);

    // ── Step 7: Submit transaction(s) via lightwalletd ────────────────────
    for tx_id in &tx_ids {
        let tx = wallet_db
            .get_transaction(*tx_id)
            .expect("get tx")
            .expect("tx exists");
        let raw_tx = {
            let mut buf = Vec::new();
            tx.write(&mut buf).expect("serialize tx");
            buf
        };

        use zcash_client_backend::proto::service::RawTransaction;

        let submit_req = RawTransaction {
            data: raw_tx,
            height: 0,
        };
        client
            .send_transaction(Request::new(submit_req))
            .await
            .expect("submit tx");
        println!("Submitted: {tx_id}");
    }

    // ── Step 8: Re-sync & check balance again ─────────────────────────────
    println!("\n=== Step 8: Re-sync & check balance ===");
    let new_tip = client
        .get_latest_block(zcash_client_backend::proto::service::ChainSpec {})
        .await
        .expect("get latest block")
        .into_inner();
    let new_tip_height = BlockHeight::from_u32(new_tip.height as u32);

    let new_blocks = download_blocks(&mut client, tip_height + 1, new_tip_height + 1).await;
    if !new_blocks.is_empty() {
        println!("Downloaded {} new blocks", new_blocks.len());
        cache_db.insert_blocks(&new_blocks);

        let last_scanned = wallet_db
            .block_fully_scanned()
            .expect("block fully scanned")
            .expect("scanned height");
        let scan_from = last_scanned.block_height() + 1;
        let from_state = ChainState::empty(last_scanned.block_height(), last_scanned.block_hash());
        let summary2 = scan_cached_blocks(
            &params,
            &cache_db,
            &mut wallet_db,
            scan_from,
            &from_state,
            usize::MAX,
        )
        .expect("re-scan blocks");
        println!(
            "Re-scanned {} blocks",
            u32::from(summary2.scanned_range().end) - u32::from(summary2.scanned_range().start)
        );
    }

    let wallet_summary_after = wallet_db
        .get_wallet_summary(Default::default())
        .expect("get wallet summary after")
        .expect("wallet is synced");
    let balance_after = wallet_summary_after
        .account_balances()
        .get(&account_id)
        .expect("account balance after");
    println!(
        "Balance after: orchard={} zat, sapling={} zat",
        balance_after.orchard_balance().total().into_u64(),
        balance_after.sapling_balance().total().into_u64(),
    );

    println!("\nDone! Reference flow complete.");
    Ok(())
}
