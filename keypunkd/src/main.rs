use std::path::PathBuf;

use clap::Parser;
use keypunkd::crypto::Keypair;
use keypunkd::protocol::ProtocolService;
use keypunkd::seed_store::FilesystemSeedStore;
use keypunkd::Keypunkd;
use paypunk_chains_ethereum::protocol::EthereumProtocol;
use paypunk_chains_zcash::protocol::ZcashProtocol;
use paypunk_ipc::IpcReceiver;
use paypunk_types::ProtocolId;
use zcash_protocol::consensus::Network;
use tactix::Actor;
use tracing::info;
use tracing_subscriber::EnvFilter;

fn default_data_dir() -> PathBuf {
    let home = std::env::var("HOME").expect("HOME must be set");
    PathBuf::from(home).join(".local/share/paypunk/")
}

#[derive(Parser)]
#[command(name = "keypunkd", about = "Key daemon for Paypunk wallet")]
struct Args {
    /// Path to the Unix domain socket
    #[arg(short, long, default_value = "/tmp/keypunkd.sock")]
    socket_path: String,

    /// Data directory for seed.enc and other state
    #[arg(short, long)]
    data_dir: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();
    let data_dir = args.data_dir.unwrap_or_else(default_data_dir);

    info!(
        socket_path = %args.socket_path,
        data_dir = %data_dir.display(),
        "keypunkd starting"
    );

    let keystore = Keypair::new();
    let (secret, public) = keystore.keypair();
    let seed_store = FilesystemSeedStore::new(data_dir.join("seed.enc").into_boxed_path());

    let mut protocols = ProtocolService::new();
    protocols.register(ProtocolId::Zcash, Box::new(ZcashProtocol {
        params: Network::MainNetwork,
    }));
    protocols.register(ProtocolId::Ethereum, Box::new(EthereumProtocol::new(())));
    info!("registered protocols: Zcash, Ethereum");

    let keypunkd = Keypunkd::new(keystore, seed_store, protocols).start();

    let server = IpcReceiver::bind_with(&args.socket_path, secret, public).await?;
    info!("keypunkd listening on {}", args.socket_path);

    let serve = tokio::spawn(async move {
        if let Err(e) = server.serve(keypunkd).await {
            tracing::error!(error = %e, "server error");
        }
    });

    tokio::signal::ctrl_c().await?;
    info!("shutting down");
    serve.abort();
    Ok(())
}
