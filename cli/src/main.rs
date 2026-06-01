use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "paypunk",
    about = "Zcash wallet for privacy-preserving commerce"
)]
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

    let client = paypunk_api::Client::connect(&cli.socket_path).await?;

    match cli.command {
        Commands::GenerateSeed { password } => {
            let password = zeroize::Zeroizing::new(password);
            let mnemonic = client.generate_seed(password).await?;
            println!("{}", *mnemonic);
        }
    }

    Ok(())
}
