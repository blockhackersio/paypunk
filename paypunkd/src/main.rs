use clap::Parser;
use keypunkd::crypto::Keypair;
use paypunk_chains_ethereum::protocol::EthereumProtocol;
use paypunk_chains_ethereum::rpc::HttpRpcClient;
use paypunk_chains_zcash::protocol::ZcashProtocol;
use paypunk_ipc::{IpcReceiver, IpcSender};
use paypunkd::protocol_service::ProtocolService;
use paypunkd::Paypunkd;
use tactix::{Actor, Sender};
use tracing::info;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "paypunkd", about = "App daemon for Paypunk wallet")]
struct Args {
    #[arg(short, long, default_value = "/tmp/paypunkd.sock")]
    socket_path: String,

    #[arg(short, long, default_value = "/tmp/keypunkd.sock")]
    keypunkd_socket: String,

    #[arg(short, long, default_value = "http://127.0.0.1:8545")]
    rpc_url: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();

    info!(
        socket_path = %args.socket_path,
        keypunkd_socket = %args.keypunkd_socket,
        "paypunkd starting"
    );

    let keystore = Keypair::new();
    let (secret, public) = keystore.keypair();

    info!("connecting to keypunkd");
    let keypunkd = IpcSender::connect(&args.keypunkd_socket).await?;
    let recipient = keypunkd.recipient();

    let zcash = ZcashProtocol {
        params: zcash_protocol::consensus::Network::MainNetwork,
    };
    let eth_client = HttpRpcClient::new(args.rpc_url.clone());
    let ethereum = EthereumProtocol::new(eth_client);
    let mut protocols = ProtocolService::new();
    protocols.register(Box::new(zcash));
    protocols.register(Box::new(ethereum));
    info!("registered protocols: Zcash, Ethereum");

    let paypunkd = Paypunkd::new(recipient, protocols).start();

    let server = IpcReceiver::bind_with(&args.socket_path, secret, public).await?;
    info!("paypunkd listening on {}", args.socket_path);

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
