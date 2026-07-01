use crate::database::Database;
use crate::paypunkd::Paypunkd;
use crate::protocol_service::ProtocolService;
use keypunkd::crypto::Keypair;
use paypunk_ipc::{IpcReceiver, IpcSender};
use paypunk_chains_zcash::wallet_actor::WalletDbActor;
use paypunk_chains_zcash::wallet_client::ZcashWalletClient;
use tactix::{Actor, Sender};
use tracing::info;
use tracing_subscriber::EnvFilter;

pub struct Config {
    pub socket_path: String,
    pub keypunkd_socket: String,
    pub ethereum_rpc_url: String,
    pub data_dir: String,
    pub lightwalletd_host: String,
    pub zcash_network: String,
}

pub async fn run(config: Config) -> Result<(), Box<dyn std::error::Error>> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .try_init();

    let keystore = Keypair::new();
    let (secret, public) = keystore.keypair();

    info!("connecting to keypunkd");
    let keypunkd = IpcSender::connect(&config.keypunkd_socket).await?;
    let recipient = keypunkd.recipient();

    // Determine Zcash network
    let zcash_params = match config.zcash_network.to_lowercase().as_str() {
        "mainnet" => zcash_protocol::consensus::Network::MainNetwork,
        "testnet" => zcash_protocol::consensus::Network::TestNetwork,
        _ => {
            tracing::warn!("unknown zcash network '{}', defaulting to testnet", config.zcash_network);
            zcash_protocol::consensus::Network::TestNetwork
        }
    };

    // Create Zcash WalletDb
    let zcash_db_dir = std::path::Path::new(&config.data_dir)
        .join("zcash")
        .join(&config.zcash_network);
    std::fs::create_dir_all(&zcash_db_dir)
        .map_err(|e| format!("failed to create zcash db dir: {e}"))?;
    let zcash_db_path = zcash_db_dir.join("wallet.db");

    let zcash_conn = rusqlite::Connection::open(&zcash_db_path)
        .map_err(|e| format!("failed to open zcash wallet db: {e}"))?;
    let wallet_db = zcash_client_sqlite::WalletDb::from_connection(
        zcash_conn,
        zcash_params,
        zcash_client_sqlite::util::SystemClock,
        rand_core::OsRng,
    );

    let wallet_actor = WalletDbActor::new(
        wallet_db, zcash_params
    ).start();
    let wallet_recipient = wallet_actor.recipient();

    let zcash_wallet_client = ZcashWalletClient {
        recipient: wallet_recipient,
    };

    let zcash = paypunk_chains_zcash::protocol::ZcashProtocol {
        params: zcash_params,
        wallet_client: Some(zcash_wallet_client),
        lightwalletd_host: Some(config.lightwalletd_host.clone()),
    };
    let eth_client = paypunk_chains_ethereum::rpc::HttpRpcClient::new(config.ethereum_rpc_url.clone());
    let ethereum = paypunk_chains_ethereum::protocol::EthereumProtocol::new(eth_client);
    let mut protocols = ProtocolService::new();
    protocols.register(Box::new(zcash));
    protocols.register(Box::new(ethereum));
    info!("registered protocols: Zcash, Ethereum");

    let db = Database::open(std::path::Path::new(&config.data_dir))
        .map_err(|e| format!("failed to open database: {e}"))?;
    info!("database opened");

    let paypunkd = Paypunkd::new(recipient, protocols, db, keystore).start();

    let server = IpcReceiver::bind_with(&config.socket_path, secret, public).await?;
    info!("paypunkd listening on {}", config.socket_path);

    let serve = tokio::spawn(async move {
        if let Err(e) = server.serve(paypunkd).await {
            tracing::error!(error = %e, "server error");
        }
    });

    tokio::signal::ctrl_c().await?;
    info!("shutting down");
    serve.abort();
    Ok(())
}
