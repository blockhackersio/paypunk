use clap::{Parser, Subcommand};
use paypunk_ipc::IpcSender;
use paypunkd::services::PaypunkService;
use tactix::Sender;
use zeroize::Zeroizing;

#[derive(Parser)]
#[command(name = "paypunk", about = "Zcash wallet for privacy-preserving commerce")]
struct Cli {
    #[arg(short, long, default_value = "/tmp/paypunkd.sock")]
    socket_path: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate a new wallet seed (initializes the wallet)
    GenerateSeed {
        /// Password used to encrypt the wallet seed
        #[arg(short, long)]
        password: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let ipc = IpcSender::connect(&cli.socket_path).await?;
    let service = PaypunkService::new(ipc.recipient());

    match cli.command {
        Commands::GenerateSeed { password } => {
            let password = Zeroizing::new(password);
            let mnemonic = paypunk_api::generate_seed(&service, password).await?;
            println!("{}", *mnemonic);
        }
    }

    Ok(())
}
