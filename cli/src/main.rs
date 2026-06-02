use clap::{Parser, Subcommand};
use paypunk_types::ProtocolId;

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
    /// Restore a wallet from an existing seed phrase
    RestoreSeed {
        /// The 12-word BIP39 mnemonic seed phrase
        #[arg(short, long)]
        mnemonic: String,
        /// Password to encrypt the restored seed
        #[arg(short, long)]
        password: String,
    },
    /// Unlock the wallet with the password
    Unlock {
        /// Wallet password
        #[arg(short, long)]
        password: String,
    },
    /// Derive an address at the given protocol, account, and diversifier index
    DeriveAddress {
        /// Protocol (zcash, bitcoin, ethereum, monero, solana)
        #[arg(short, long, default_value = "zcash")]
        protocol: String,
        /// Account index (default: 0)
        #[arg(short, long, default_value_t = 0)]
        account: u32,
        /// Diversifier / address index (default: 0)
        #[arg(short, long, default_value_t = 0)]
        index: u32,
    },
    /// Lock the wallet (zeroize in-memory seed)
    Lock,
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
        Commands::RestoreSeed { mnemonic, password } => {
            let mnemonic = zeroize::Zeroizing::new(mnemonic);
            let password = zeroize::Zeroizing::new(password);
            client.restore_seed(mnemonic, password).await?;
            println!("Seed restored successfully");
        }
        Commands::Unlock { password } => {
            let password = zeroize::Zeroizing::new(password);
            client.unlock(password).await?;
            println!("Wallet unlocked");
        }
        Commands::DeriveAddress {
            protocol,
            account,
            index,
        } => {
            let protocol_id = match protocol.to_lowercase().as_str() {
                "zcash" => ProtocolId::Zcash,
                "bitcoin" => ProtocolId::Bitcoin,
                "ethereum" => ProtocolId::Ethereum,
                "monero" => ProtocolId::Monero,
                "solana" => ProtocolId::Solana,
                _ => {
                    eprintln!("Unknown protocol: {protocol}");
                    std::process::exit(1);
                }
            };
            let address = client.derive_address(protocol_id, account, index).await?;
            println!("{address}");
        }
        Commands::Lock => {
            client.lock().await?;
            println!("Wallet locked");
        }
    }

    Ok(())
}
