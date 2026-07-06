pub mod address;
pub mod lsp_client;
pub mod protocol;
pub mod wallet_actor;

use std::path::Path;

use tactix::{Actor, Recipient, Sender};
use zcash_client_backend::data_api::wallet::ConfirmationsPolicy;
use zcash_protocol::consensus::NetworkType;

pub use protocol::ZcashProtocol;
pub use wallet_actor::{WalletDbActor, WalletMessage};

/// Patch the orchard shard scan range views for regtest, where all upgrades activate
/// at block 1 but the zcash_protocol crate's TestNetwork has NU5 at 1842420.
fn patch_orchard_views_for_regtest(db_path: &Path) -> Result<(), String> {
    let conn = rusqlite::Connection::open(db_path)
        .map_err(|e| format!("failed to open db for view patch: {e}"))?;
    conn.execute_batch(
        "DROP VIEW IF EXISTS v_orchard_shards_scan_state;
         DROP VIEW IF EXISTS v_orchard_shard_unscanned_ranges;
         DROP VIEW IF EXISTS v_orchard_shard_scan_ranges;

         CREATE VIEW v_orchard_shard_scan_ranges AS
         SELECT
             shard.shard_index,
             shard.shard_index << 16 AS start_position,
             (shard.shard_index + 1) << 16 AS end_position_exclusive,
             IFNULL(prev_shard.subtree_end_height, 0) AS subtree_start_height,
             shard.subtree_end_height,
             shard.contains_marked,
             scan_queue.block_range_start,
             scan_queue.block_range_end,
             scan_queue.priority
         FROM orchard_tree_shards shard
         LEFT OUTER JOIN orchard_tree_shards prev_shard
             ON shard.shard_index = prev_shard.shard_index + 1
         INNER JOIN scan_queue ON (
             IFNULL(prev_shard.subtree_end_height, 0) < scan_queue.block_range_end AND
             (
                 scan_queue.block_range_start <= shard.subtree_end_height OR
                 shard.subtree_end_height IS NULL
             )
         );

         CREATE VIEW v_orchard_shard_unscanned_ranges AS
         WITH wallet_birthday AS (SELECT MIN(birthday_height) AS height FROM accounts)
         SELECT
             shard_index, start_position, end_position_exclusive,
             subtree_start_height, subtree_end_height, contains_marked,
             block_range_start, block_range_end, priority
         FROM v_orchard_shard_scan_ranges
         INNER JOIN wallet_birthday
         WHERE priority > 10
         AND block_range_end > wallet_birthday.height;

         CREATE VIEW v_orchard_shards_scan_state AS
         SELECT
             shard_index, start_position, end_position_exclusive,
             subtree_start_height, subtree_end_height, contains_marked,
             MAX(priority) AS max_priority
         FROM v_orchard_shard_scan_ranges
         GROUP BY
             shard_index, start_position, end_position_exclusive,
             subtree_start_height, subtree_end_height, contains_marked;",
    )
    .map_err(|e| format!("failed to patch orchard views: {e}"))?;
    Ok(())
}

/// Open (or create) the wallet database, retrying once if the database is stale.
///
/// If the database file exists but is in a bad state (corrupted, readonly from a
/// stale WAL, etc.), we delete it and start fresh. This is safe because the
/// wallet DB only contains scanned chain data — keys live in keypunkd.
fn open_wallet_db(
    db_path: &Path,
    params: zcash_protocol::consensus::Network,
    network_type: NetworkType,
) -> Result<
    zcash_client_sqlite::WalletDb<
        rusqlite::Connection,
        zcash_protocol::consensus::Network,
        zcash_client_sqlite::util::SystemClock,
        rand_core::OsRng,
    >,
    String,
> {
    let maybe_db = (|| -> Result<_, String> {
        let mut db = zcash_client_sqlite::WalletDb::for_path(
            db_path,
            params,
            zcash_client_sqlite::util::SystemClock,
            rand_core::OsRng,
        )
        .map_err(|e| format!("{e}"))?;
        zcash_client_sqlite::wallet::init::init_wallet_db(&mut db, None)
            .map_err(|e| format!("{e}"))?;

        if network_type == NetworkType::Regtest {
            patch_orchard_views_for_regtest(db_path)?;
        }

        Ok(db)
    })();

    match maybe_db {
        Ok(db) => Ok(db),
        Err(e) => {
            // If the DB file exists, a database-level error means it's stale.
            // Delete and retry once.
            if db_path.exists() {
                tracing::warn!("wallet DB is stale, deleting and recreating: {e}");
                let _ = std::fs::remove_file(db_path);
                let _ = std::fs::remove_file(db_path.with_extension("db-wal"));
                let _ = std::fs::remove_file(db_path.with_extension("db-shm"));

                let mut db = zcash_client_sqlite::WalletDb::for_path(
                    db_path,
                    params,
                    zcash_client_sqlite::util::SystemClock,
                    rand_core::OsRng,
                )
                .map_err(|e| format!("failed to open zcash wallet db: {e}"))?;
                zcash_client_sqlite::wallet::init::init_wallet_db(&mut db, None)
                    .map_err(|e| format!("failed to initialize zcash wallet db: {e}"))?;
                if network_type == NetworkType::Regtest {
                    patch_orchard_views_for_regtest(db_path)?;
                }
                Ok(db)
            } else {
                Err(format!("failed to open zcash wallet db: {e}"))
            }
        }
    }
}

/// Create a fully-initialized Zcash protocol with a running WalletDbActor.
pub async fn create_protocol(
    data_dir: &Path,
    lightwalletd_host: String,
    zcash_network: &str,
) -> Result<ZcashProtocol, String> {
    let (params, network_type) = match zcash_network.to_lowercase().as_str() {
        "mainnet" => (
            zcash_protocol::consensus::Network::MainNetwork,
            zcash_protocol::consensus::NetworkType::Main,
        ),
        "testnet" => (
            zcash_protocol::consensus::Network::TestNetwork,
            zcash_protocol::consensus::NetworkType::Test,
        ),
        "regtest" => (
            zcash_protocol::consensus::Network::TestNetwork,
            zcash_protocol::consensus::NetworkType::Regtest,
        ),
        _ => {
            tracing::warn!(
                "unknown zcash network '{}', defaulting to regtest",
                zcash_network
            );
            (
                zcash_protocol::consensus::Network::TestNetwork,
                zcash_protocol::consensus::NetworkType::Regtest,
            )
        }
    };

    let zcash_db_dir = data_dir.join("zcash").join(zcash_network);
    std::fs::create_dir_all(&zcash_db_dir)
        .map_err(|e| format!("failed to create zcash db dir: {e}"))?;
    let zcash_db_path = zcash_db_dir.join("wallet.db");

    let wallet_db = open_wallet_db(&zcash_db_path, params, network_type)?;

    let confirmations = match network_type {
        zcash_protocol::consensus::NetworkType::Regtest => ConfirmationsPolicy::MIN,
        _ => ConfirmationsPolicy::default(),
    };
    let wallet_actor = WalletDbActor::new(wallet_db, params, network_type, zcash_db_path, confirmations).start();
    let recipient: Recipient<WalletMessage> = wallet_actor.clone().recipient();

    let protocol = ZcashProtocol::new(
        params,
        network_type,
        Some(recipient),
        Some(lightwalletd_host),
        Some("http://127.0.0.1:18232".to_string()),
    );

    Ok(protocol)
}

/// Return the standard Zcash derivation path for a given account index.
///
/// Zcash uses ZIP32 for per-account key derivation. The path identifies the
/// account; addresses are derived from the resulting `UnifiedSpendingKey`
/// using diversifier indices (not BIP44 address-level indices).
///
/// Path: `m/44'/133'/{account}'`
pub fn derivation_path(account: u32) -> String {
    format!("m/44'/133'/{account}'")
}
