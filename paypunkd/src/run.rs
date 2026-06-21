use crate::database::Database;
use crate::paypunkd::Paypunkd;
use crate::protocol_service::ProtocolService;
use keypunkd::crypto::Keypair;
use paypunk_ipc::{IpcReceiver, IpcSender};
use tactix::{Actor, Sender};
use tracing::info;
use tracing_subscriber::EnvFilter;

pub struct Config {
    pub socket_path: String,
    pub keypunkd_socket: String,
    pub rpc_url: String,
    pub data_dir: String,
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

    let zcash = paypunk_chains_zcash::protocol::ZcashProtocol {
        params: zcash_protocol::consensus::Network::MainNetwork,
    };
    let eth_client = paypunk_chains_ethereum::rpc::HttpRpcClient::new(config.rpc_url.clone());
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
