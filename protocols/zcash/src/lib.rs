pub mod address;
pub mod lsp_client;
pub mod protocol;
pub mod wallet_actor;

use std::path::Path;

use tactix::{Actor, Recipient, Sender};

pub use protocol::ZcashProtocol;
pub use wallet_actor::{WalletDbActor, WalletMessage};

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

    let zcash_conn = rusqlite::Connection::open(&zcash_db_path)
        .map_err(|e| format!("failed to open zcash wallet db: {e}"))?;
    let wallet_db = zcash_client_sqlite::WalletDb::from_connection(
        zcash_conn,
        params,
        zcash_client_sqlite::util::SystemClock,
        rand_core::OsRng,
    );

    let wallet_actor = WalletDbActor::new(wallet_db, params, zcash_db_path).start();
    let recipient: Recipient<WalletMessage> = wallet_actor.clone().recipient();

    let protocol = ZcashProtocol::new(
        params,
        Some(recipient),
        Some(lightwalletd_host),
        Some(wallet_actor),
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
