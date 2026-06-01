use std::path::PathBuf;

use clap::Parser;
use keypunkd::crypto::KeyStore;
use keypunkd::dispatcher::Dispatcher;
use keypunkd::seed_store::FilesystemSeedStore;
use paypunk_ipc::IpcReceiver;
use tactix::Actor;
use tokio::net::UnixListener;

#[derive(Parser)]
#[command(name = "keypunkd", about = "Key daemon for Paypunk wallet")]
struct Args {
    /// Path to the Unix domain socket
    #[arg(short, long, default_value = "/tmp/keypunkd.sock")]
    socket_path: String,

    /// Data directory for seed.enc and other state
    #[arg(short, long, default_value = "/tmp/paypunk/data")]
    data_dir: PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let keystore = KeyStore::new();
    let (secret, public) = keystore.keypair();
    let seed_store = FilesystemSeedStore::new(args.data_dir.join("seed.enc").into_boxed_path());

    let dispatcher = Dispatcher::new(keystore, seed_store).start();

    let socket_path = &args.socket_path;
    if std::path::Path::new(socket_path).exists() {
        std::fs::remove_file(socket_path)?;
    }
    let listener = UnixListener::bind(socket_path)?;
    let server = IpcReceiver::new(listener, secret, public);
    eprintln!("keypunkd listening on {}", socket_path);

    let serve = tokio::spawn(async move {
        if let Err(e) = server.serve(dispatcher).await {
            eprintln!("server error: {e}");
        }
    });

    tokio::signal::ctrl_c().await?;
    eprintln!("shutting down");
    serve.abort();
    Ok(())
}
