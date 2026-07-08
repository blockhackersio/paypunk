use clap::Parser;
use paypunk_ipc::{IpcMessage, IpcSender};
use tactix::Sender;

#[derive(Parser)]
#[command(name = "paypunk-ping")]
struct Cli {
    #[arg(long, default_value = "/tmp/keypunkd.sock")]
    socket_path: String,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    println!("Connecting to {}...", cli.socket_path);

    let addr = match IpcSender::connect(&cli.socket_path).await {
        Ok(addr) => addr,
        Err(e) => {
            eprintln!("❌ Failed to connect: {e}");
            std::process::exit(1);
        }
    };

    println!("Sending ping...");

    let result = addr.ask(IpcMessage::new(b"ping".to_vec())).await;

    match result {
        Ok(bytes) => {
            if bytes == b"pong" {
                println!("✅ Pong received");
            } else {
                eprintln!(
                    "❌ Unexpected response: {}",
                    String::from_utf8_lossy(&bytes)
                );
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("❌ Error: {e}");
            std::process::exit(1);
        }
    }
}
