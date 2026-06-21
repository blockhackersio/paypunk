use blake2::digest::consts::U32;
use blake2::{Blake2b, Digest};
use clap::{Parser, Subcommand};
use paypunk_api::Client;
use paypunk_config::ConfigLoader;
use paypunk_types::{
    ArtifactSummary, AssetId, EthereumIntent, Intent, ProtocolId, ZcashIntent,
};
use paypunk_tui::run_tui;
use std::path::Path;
use std::process::{exit, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use zeroize::Zeroizing;

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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let config = ConfigLoader::load_or_default();
    let socket_path = cli
        .socket_path
        .clone()
        .unwrap_or(config.paypunkd_socket_path);

    match cli.command {
        None => {
            let config = ConfigLoader::load_or_default();
            let exe = std::env::current_exe()
                .map_err(|e| format!("Failed to get current exe path: {e}"))?;
            let paypunkd_socket = cli.socket_path.unwrap_or(config.paypunkd_socket_path);
            let keypunkd_socket = config.keypunkd_socket_path.clone();
            let data_dir = config.data_dir.clone();
            let rpc_url = config.rpc_url.clone();

            let mut keypunkd_child = Command::new(&exe)
                .arg("keypunkd")
                .arg("--socket-path")
                .arg(&keypunkd_socket)
                .arg("--data-dir")
                .arg(&data_dir)
                .spawn()
                .map_err(|e| format!("Failed to spawn keypunkd: {e}"))?;

            let mut paypunkd_child = Command::new(&exe)
                .arg("paypunkd")
                .arg("--socket-path")
                .arg(&paypunkd_socket)
                .arg("--keypunkd-socket")
                .arg(&keypunkd_socket)
                .arg("--rpc-url")
                .arg(&rpc_url)
                .arg("--data-dir")
                .arg(&data_dir)
                .spawn()
                .map_err(|e| format!("Failed to spawn paypunkd: {e}"))?;

            let wait_result = tokio::time::timeout(
                Duration::from_secs(30),
                wait_for_sockets(&[&keypunkd_socket, &paypunkd_socket]),
            )
            .await;

            if wait_result.is_err() {
                let _ = keypunkd_child.kill();
                let _ = paypunkd_child.kill();
                let _ = keypunkd_child.wait();
                let _ = paypunkd_child.wait();
                return Err("Timed out waiting for daemon sockets to appear".into());
            }

            let shutdown = Arc::new(AtomicBool::new(false));
            let shutdown_clone = shutdown.clone();
            tokio::spawn(async move {
                tokio::signal::ctrl_c().await.ok();
                shutdown_clone.store(true, Ordering::SeqCst);
            });

            let tui_result = run_tui(&paypunkd_socket, Some(shutdown)).await;

            let _ = keypunkd_child.kill();
            let _ = paypunkd_child.kill();
            let _ = keypunkd_child.wait();
            let _ = paypunkd_child.wait();

            tui_result.map_err(|e| e.into())
        }
        Some(Commands::Tui) => {
            run_tui(&socket_path, None).await.map_err(|e| e.into())
        }
        Some(Commands::Keypunkd {
            socket_path,
            data_dir,
        }) => {
            let config = ConfigLoader::load_or_default();
            let socket = socket_path.unwrap_or(config.keypunkd_socket_path);
            let dir = data_dir.unwrap_or(config.data_dir);

            keypunkd::run::run(keypunkd::run::Config {
                socket_path: socket,
                data_dir: dir,
            })
            .await
        }
        Some(Commands::Paypunkd {
            socket_path,
            keypunkd_socket,
            rpc_url,
            data_dir,
        }) => {
            let config = ConfigLoader::load_or_default();
            let socket = socket_path.unwrap_or(config.paypunkd_socket_path);
            let ks = keypunkd_socket.unwrap_or(config.keypunkd_socket_path);
            let url = rpc_url.unwrap_or(config.rpc_url);
            let dir = data_dir.unwrap_or(config.data_dir);

            paypunkd::run::run(paypunkd::run::Config {
                socket_path: socket,
                keypunkd_socket: ks,
                rpc_url: url,
                data_dir: dir,
            })
            .await
        }
        Some(command) => async_main(socket_path, command).await,
    }
}

async fn async_main(
    socket_path: String,
    command: Commands,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::connect(&socket_path).await?;

    match command {
        Commands::GenerateSeed { password } => {
            let password = Zeroizing::new(password);
            let mnemonic = client.generate_seed(password).await?;
            println!("{}", *mnemonic);
        }
        Commands::RestoreSeed { mnemonic, password } => {
            let mnemonic = Zeroizing::new(mnemonic);
            let password = Zeroizing::new(password);
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
        Commands::ApproveSignature {
            password: _password,
            account,
        } => {
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
                    exit(1);
                }
            };
            let asset = AssetId::Native;
            let balance = client
                .get_balance_legacy(protocol_id, account, asset)
                .await?;
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

async fn wait_for_sockets(paths: &[&str]) {
    loop {
        let all_exist = paths.iter().all(|p| Path::new(p).exists());
        if all_exist {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

async fn submit_intent_flow(
    client: &paypunk_api::Client,
    intent: Intent,
    derivation_path: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Submitting intent for preview...");
    match client.submit_intent(intent, derivation_path).await {
        Ok((raw_artifact, parsed_summary, keypunkd_signature, keypunkd_public_key)) => {
            // Verify the signature: H(raw, parsed, path) should match
            // In a production build, we'd verify against keypunkd's public key
            let mut to_verify = Vec::new();
            to_verify.extend_from_slice(&raw_artifact);
            to_verify.extend_from_slice(&parsed_summary);
            to_verify.extend_from_slice(derivation_path);
            let _hash = Blake2b::<U32>::digest(&to_verify);

            println!("Artifact preview received:");
            println!("  Raw artifact: {} bytes", raw_artifact.len());

            // Try to deserialize the parsed summary
            if let Ok(summary) =
                postcard::from_bytes::<ArtifactSummary>(&parsed_summary)
            {
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
            exit(1);
        }
    }
}
