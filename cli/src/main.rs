use clap::{Parser, Subcommand};
use paypunk_types::{EthereumIntent, Intent, ProtocolId, ZcashIntent};
use paypunk_config::ConfigLoader;
use blake2::Digest;
use std::process::{Child, Command};
use std::path::Path;
use std::time::Duration;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Parser)]
#[command(
    name = "paypunk",
    about = "Zcash wallet for privacy-preserving commerce"
)]
struct Cli {
    #[arg(short, long)]
    socket_path: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate a new wallet seed (initializes the wallet)
    GenerateSeed {
        #[arg(short, long)]
        password: String,
    },
    /// Restore a wallet from an existing seed phrase
    RestoreSeed {
        #[arg(short, long)]
        mnemonic: String,
        #[arg(short, long)]
        password: String,
    },
    /// Submit a Zcash transfer intent for preview
    SubmitZcashTransfer {
        #[arg(short, long)]
        to: String,
        #[arg(short, long)]
        amount: String,
        #[arg(short, long)]
        from: String,
        #[arg(short, long, default_value = "zcash:mainnet/slip44:133")]
        asset: String,
        #[arg(short, long)]
        memo: Option<String>,
        #[arg(short, long, default_value_t = 0)]
        account: u32,
    },
    /// Submit an Ethereum transfer intent for preview
    SubmitEthTransfer {
        #[arg(short, long)]
        to: String,
        #[arg(short, long)]
        amount: String,
        #[arg(short, long)]
        from: String,
        #[arg(short, long, default_value = "eip155:1/slip44:60")]
        asset: String,
        #[arg(short, long)]
        data: Option<String>,
        #[arg(short, long, default_value_t = 0)]
        account: u32,
    },
    /// Approve a previously submitted intent by providing the password
    ApproveSignature {
        /// Password to authorize the signing
        #[arg(short, long)]
        password: String,
        #[arg(short, long, default_value_t = 0)]
        account: u32,
    },
    /// Query the balance for a protocol and account
    GetBalance {
        #[arg(short, long, default_value = "zcash")]
        protocol: String,
        #[arg(short, long, default_value_t = 0)]
        account: u32,
    },
    /// Launch the terminal user interface
    Tui,
    /// Launch keypunkd (key daemon) as a child process
    Keypunkd {
        #[arg(short, long)]
        socket_path: Option<String>,
        #[arg(short, long)]
        data_dir: Option<String>,
    },
    /// Launch paypunkd (app daemon) as a child process
    Paypunkd {
        #[arg(short, long)]
        socket_path: Option<String>,
        #[arg(short, long)]
        keypunkd_socket: Option<String>,
        #[arg(short, long)]
        rpc_url: Option<String>,
        #[arg(short, long)]
        data_dir: Option<String>,
    },
}

struct DaemonProcess {
    keypunkd: Child,
    paypunkd: Child,
}

async fn spawn_daemons() -> Result<DaemonProcess, Box<dyn std::error::Error>> {
    let config = ConfigLoader::load_or_default();

    let keypunkd = Command::new("keypunkd")
        .spawn()
        .map_err(|e| format!("Failed to start keypunkd: {e}"))?;

    let paypunkd = Command::new("paypunkd")
        .spawn()
        .map_err(|e| format!("Failed to start paypunkd: {e}"))?;

    wait_for_socket(&config.keypunkd_socket_path, Duration::from_secs(10)).await?;
    wait_for_socket(&config.paypunkd_socket_path, Duration::from_secs(10)).await?;

    Ok(DaemonProcess { keypunkd, paypunkd })
}

async fn wait_for_socket(path: &str, timeout: Duration) -> Result<(), String> {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if Path::new(path).exists() {
            if let Ok(_stream) = tokio::net::UnixStream::connect(path).await {
                return Ok(());
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(format!("socket {path} did not become ready within timeout"))
}

fn kill_daemons(daemons: &mut DaemonProcess) {
    let _ = daemons.keypunkd.kill();
    let _ = daemons.paypunkd.kill();
    let _ = daemons.keypunkd.wait();
    let _ = daemons.paypunkd.wait();
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let config = ConfigLoader::load_or_default();
    let socket_path = cli.socket_path.unwrap_or(config.paypunkd_socket_path);

    match cli.command {
        None | Some(Commands::Tui) => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(async {
                let shutdown = Arc::new(AtomicBool::new(false));
                let shutdown_clone = shutdown.clone();
                tokio::spawn(async move {
                    tokio::signal::ctrl_c().await.ok();
                    shutdown_clone.store(true, Ordering::SeqCst);
                });

                let mut daemons = spawn_daemons().await?;

                let result = paypunk_tui::run_tui(&socket_path, Some(shutdown)).await;

                kill_daemons(&mut daemons);
                result.map_err(|e| e.into())
            })
        }
        Some(Commands::Keypunkd { socket_path, data_dir }) => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(async {
                let config = ConfigLoader::load_or_default();
                let socket = socket_path.unwrap_or(config.keypunkd_socket_path);
                let dir = data_dir.unwrap_or(config.data_dir);

                let mut child = Command::new("keypunkd")
                    .arg("--socket-path")
                    .arg(&socket)
                    .arg("--data-dir")
                    .arg(&dir)
                    .spawn()
                    .map_err(|e| format!("Failed to start keypunkd: {e}"))?;

                let shutdown = Arc::new(AtomicBool::new(false));
                let shutdown_clone = shutdown.clone();
                tokio::spawn(async move {
                    tokio::signal::ctrl_c().await.ok();
                    shutdown_clone.store(true, Ordering::SeqCst);
                });

                while !shutdown.load(Ordering::SeqCst) {
                    if let Ok(Some(_)) = child.try_wait() {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }

                let _ = child.kill();
                let _ = child.wait();
                Ok(())
            })
        }
        Some(Commands::Paypunkd {
            socket_path,
            keypunkd_socket,
            rpc_url,
            data_dir,
        }) => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(async {
                let config = ConfigLoader::load_or_default();
                let socket = socket_path.unwrap_or(config.paypunkd_socket_path);
                let ks = keypunkd_socket.unwrap_or(config.keypunkd_socket_path);
                let url = rpc_url.unwrap_or(config.rpc_url);
                let dir = data_dir.unwrap_or(config.data_dir);

                let mut child = Command::new("paypunkd")
                    .arg("--socket-path")
                    .arg(&socket)
                    .arg("--keypunkd-socket")
                    .arg(&ks)
                    .arg("--rpc-url")
                    .arg(&url)
                    .arg("--data-dir")
                    .arg(&dir)
                    .spawn()
                    .map_err(|e| format!("Failed to start paypunkd: {e}"))?;

                let shutdown = Arc::new(AtomicBool::new(false));
                let shutdown_clone = shutdown.clone();
                tokio::spawn(async move {
                    tokio::signal::ctrl_c().await.ok();
                    shutdown_clone.store(true, Ordering::SeqCst);
                });

                while !shutdown.load(Ordering::SeqCst) {
                    if let Ok(Some(_)) = child.try_wait() {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }

                let _ = child.kill();
                let _ = child.wait();
                Ok(())
            })
        }
        Some(command) => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(async_main(socket_path, command))
        }
    }
}

async fn async_main(socket_path: String, command: Commands) -> Result<(), Box<dyn std::error::Error>> {
    let client = paypunk_api::Client::connect(&socket_path).await?;

    match command {
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
        Commands::SubmitZcashTransfer {
            to,
            amount,
            from,
            asset,
            memo,
            account,
        } => {
            let intent = Intent::Zcash(ZcashIntent::Transfer {
                to,
                amount,
                from,
                asset,
                memo,
            });
            let path = account.to_le_bytes();
            submit_intent_flow(&client, intent, &path).await?;
        }
        Commands::SubmitEthTransfer {
            to,
            amount,
            from,
            asset,
            data,
            account,
        } => {
            let intent = Intent::Ethereum(EthereumIntent::Transfer {
                to,
                amount,
                from,
                asset,
                data,
            });
            let path = account.to_le_bytes();
            submit_intent_flow(&client, intent, &path).await?;
        }
        Commands::ApproveSignature { password: _password, account } => {
            let _path = account.to_le_bytes();
            println!("Approving signature for account {account}...");
            // In a real app, the preview data would be stored in state between
            // submit and approve. For now this is a placeholder.
            println!("ApproveSignature must be used interactively after SubmitIntent");
            println!("Re-run with a Submit* command first");
        }
        Commands::GetBalance { protocol, account } => {
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
            let asset = paypunk_types::AssetId::Native;
            let balance = client.get_balance_legacy(protocol_id, account, asset).await?;
            println!(
                "Balance (protocol={protocol}, account={account}): spendable={}, pending={}, total={}",
                balance.spendable.0,
                balance.pending.0,
                balance.total.0,
            );
        }
        Commands::Tui => unreachable!(),
        Commands::Keypunkd { .. } => unreachable!(),
        Commands::Paypunkd { .. } => unreachable!(),
    }

    Ok(())
}

async fn submit_intent_flow(client: &paypunk_api::Client, intent: Intent, derivation_path: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    println!("Submitting intent for preview...");
    match client.submit_intent(intent, derivation_path).await {
        Ok((raw_artifact, parsed_summary, keypunkd_signature, keypunkd_public_key)) => {
            // Verify the signature: H(raw, parsed, path) should match
            // In a production build, we'd verify against keypunkd's public key
            let mut to_verify = Vec::new();
            to_verify.extend_from_slice(&raw_artifact);
            to_verify.extend_from_slice(&parsed_summary);
            to_verify.extend_from_slice(derivation_path);
            let _hash = blake2::Blake2b::<blake2::digest::consts::U32>::digest(&to_verify);

            println!("Artifact preview received:");
            println!("  Raw artifact: {} bytes", raw_artifact.len());

            // Try to deserialize the parsed summary
            if let Ok(summary) = postcard::from_bytes::<paypunk_types::ArtifactSummary>(&parsed_summary) {
                println!("  To: {}", summary.to);
                println!("  Amount: {}", summary.amount);
                println!("  Fee: {}", summary.fee);
                if let Some(memo) = summary.memo {
                    println!("  Memo: {memo}");
                }
                println!("  Protocol: {:?}", summary.protocol);
            } else {
                println!("  Parsed summary: {} bytes (raw)", parsed_summary.len());
            }

            println!("  Signature: {} bytes", keypunkd_signature.len());
            println!("  Keypunkd public key: {:?}", keypunkd_public_key);

            // Store the preview data for approval (in a real app, we'd use state)
            // For now, prompt the user to approve
            println!();
            println!("To approve, run: paypunk approve-signature --password <your-password>");
            println!("(In a future version, this will be an interactive prompt)");
            Ok(())
        }
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }
}
