pub mod address;
pub mod lsp_client;
pub mod protocol;
pub mod wallet_actor;

use std::path::Path;

use tactix::{Actor, Recipient, Sender};

pub use protocol::ZcashProtocol;
pub use wallet_actor::{WalletDbActor, WalletMessage};

/// Open (or create) the wallet database, retrying once if the database is stale.
///
/// If the database file exists but is in a bad state (corrupted, readonly from a
/// stale WAL, etc.), we delete it and start fresh. This is safe because the
/// wallet DB only contains scanned chain data — keys live in keypunkd.
fn open_wallet_db(
    db_path: &Path,
    params: zcash_protocol::consensus::Network,
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
    let params = match zcash_network.to_lowercase().as_str() {
        "mainnet" => zcash_protocol::consensus::Network::MainNetwork,
        "testnet" => zcash_protocol::consensus::Network::TestNetwork,
        _ => {
            tracing::warn!(
                "unknown zcash network '{}', defaulting to testnet",
                zcash_network
            );
            zcash_protocol::consensus::Network::TestNetwork
        }
    };

    let zcash_db_dir = data_dir.join("zcash").join(zcash_network);
    std::fs::create_dir_all(&zcash_db_dir)
        .map_err(|e| format!("failed to create zcash db dir: {e}"))?;
    let zcash_db_path = zcash_db_dir.join("wallet.db");

    let wallet_db = open_wallet_db(&zcash_db_path, params)?;

    let wallet_actor = WalletDbActor::new(wallet_db, params, zcash_db_path).start();
    let recipient: Recipient<WalletMessage> = wallet_actor.clone().recipient();

    let protocol = ZcashProtocol::new(params, Some(recipient), Some(lightwalletd_host));

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
