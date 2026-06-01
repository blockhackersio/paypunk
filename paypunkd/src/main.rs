use clap::Parser;
use tokio::net::UnixListener;

#[derive(Parser)]
#[command(name = "paypunkd", about = "App daemon for Paypunk wallet")]
struct Args {
    #[arg(short, long, default_value = "/tmp/paypunkd.sock")]
    socket_path: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let socket_path = &args.socket_path;
    if std::path::Path::new(socket_path).exists() {
        std::fs::remove_file(socket_path)?;
    }
    let _listener = UnixListener::bind(socket_path)?;

    // TODO: Generate keypair, create IpcReceiver, wire Dispatcher with
    // an IpcSender connected to keypunkd as the KeypunkService recipient.
    eprintln!("paypunkd skeleton — wire KeypunkService with IPC to keypunkd");

    Ok(())
}
