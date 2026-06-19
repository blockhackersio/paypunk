use clap::Parser;
use keypunkd::crypto::Keypair;
use paypunk_chains_ethereum::protocol::EthereumProtocol;
use paypunk_chains_ethereum::rpc::HttpRpcClient;
use paypunk_chains_zcash::protocol::ZcashProtocol;
use paypunk_ipc::{IpcReceiver, IpcSender};
use paypunkd::config::{ConfigSource, HardcodedConfig};
use paypunkd::protocol_service::ProtocolService;
use paypunkd::Paypunkd;
use tactix::{Actor, Sender};
use tracing::info;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "paypunkd", about = "App daemon for Paypunk wallet")]
struct Args {
    #[arg(short, long)]
    socket_path: Option<String>,

    #[arg(short, long)]
    keypunkd_socket: Option<String>,

    #[arg(short, long)]
    rpc_url: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();
    let config = HardcodedConfig;

    let socket_path = args.socket_path.unwrap_or_else(|| config.paypunkd_socket_path().to_string());
    let keypunkd_socket = args.keypunkd_socket.unwrap_or_else(|| config.keypunkd_socket_path().to_string());
    let rpc_url = args.rpc_url.unwrap_or_else(|| config.rpc_url().to_string());

    info!(
        socket_path = %socket_path,
        keypunkd_socket = %keypunkd_socket,
        "paypunkd starting"
    );

    let keystore = Keypair::new();
    let (secret, public) = keystore.keypair();

    info!("connecting to keypunkd");
    let keypunkd = IpcSender::connect(&keypunkd_socket).await?;
    let recipient = keypunkd.recipient();

    let zcash = ZcashProtocol {
        params: zcash_protocol::consensus::Network::MainNetwork,
    };
    let eth_client = HttpRpcClient::new(rpc_url.clone());
    let ethereum = EthereumProtocol::new(eth_client);
    let mut protocols = ProtocolService::new();
    protocols.register(Box::new(zcash));
    protocols.register(Box::new(ethereum));
    info!("registered protocols: Zcash, Ethereum");

    let paypunkd = Paypunkd::new(recipient, protocols).start();

    let server = IpcReceiver::bind_with(&socket_path, secret, public).await?;
    info!("paypunkd listening on {}", socket_path);

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
