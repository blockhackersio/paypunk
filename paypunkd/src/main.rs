use clap::Parser;
use keypunkd::crypto::Keypair;
use paypunk_ipc::{IpcReceiver, IpcSender};
use paypunkd::dispatcher::Dispatcher;
use tactix::{Actor, Sender};
use tokio::net::UnixListener;

#[derive(Parser)]
#[command(name = "paypunkd", about = "App daemon for Paypunk wallet")]
struct Args {
    #[arg(short, long, default_value = "/tmp/paypunkd.sock")]
    socket_path: String,

    #[arg(short, long, default_value = "/tmp/keypunkd.sock")]
    keypunkd_socket: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let keystore = Keypair::new();
    let (secret, public) = keystore.keypair();

    let keypunkd = IpcSender::connect(&args.keypunkd_socket).await?;
    let recipient = keypunkd.recipient();

    let dispatcher = Dispatcher::new(recipient).start();

    let socket_path = &args.socket_path;
    if std::path::Path::new(socket_path).exists() {
        std::fs::remove_file(socket_path)?;
    }
    let listener = UnixListener::bind(socket_path)?;
    let server = IpcReceiver::new(listener, secret, public);
    eprintln!("paypunkd listening on {}", socket_path);

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
