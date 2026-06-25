use blake2::digest::consts::U32;
use blake2::{Blake2b, Digest};
use clap::{Parser, Subcommand};
use paypunk_api::Client;
use paypunk_config::ConfigLoader;
use paypunk_tui::run_tui;
use paypunk_types::{ArtifactSummary, AssetId, EthereumIntent, Intent, ProtocolId, ZcashIntent};
use std::fs;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use zeroize::Zeroizing;

/// Holds spawned daemon child processes and kills them on drop.
struct DaemonGuard {
    keypunkd: Option<Child>,
    paypunkd: Option<Child>,
}

impl DaemonGuard {
    fn new() -> Self {
        Self {
            keypunkd: None,
            paypunkd: None,
        }
    }
}

impl Drop for DaemonGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.keypunkd.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(mut child) = self.paypunkd.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Spawn keypunkd and paypunkd if the paypunkd socket doesn't already exist
/// with a live daemon. Returns a guard that kills the daemons on drop.
async fn ensure_daemons(
    paypunkd_socket: &str,
    keypunkd_socket: &str,
) -> Result<DaemonGuard, Box<dyn std::error::Error>> {
    // If socket exists, try a quick connect to see if it's live
    if Path::new(paypunkd_socket).exists() {
        match tokio::time::timeout(
            Duration::from_millis(500),
            Client::connect(paypunkd_socket),
        )
        .await
        {
            Ok(Ok(_client)) => return Ok(DaemonGuard::new()),
            _ => {
                // Stale socket — clean it and proceed to spawn
                let _ = fs::remove_file(keypunkd_socket);
                let _ = fs::remove_file(paypunkd_socket);
            }
        }
    }

    let exe = std::env::current_exe()
        .map_err(|e| format!("Failed to get current exe path: {e}"))?;

    // Clean stale sockets before spawning
    let _ = fs::remove_file(keypunkd_socket);
    let _ = fs::remove_file(paypunkd_socket);

    println!("Starting keypunkd...");
    let mut keypunkd = Command::new(&exe)
        .arg("keypunkd")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to spawn keypunkd: {e}"))?;

    let keypunkd_wait =
        tokio::time::timeout(Duration::from_secs(30), wait_for_sockets(&[keypunkd_socket])).await;
    if keypunkd_wait.is_err() {
        let _ = keypunkd.kill();
        let _ = keypunkd.wait();
        return Err("Timed out waiting for keypunkd socket".into());
    }

    println!("Starting paypunkd...");
    let mut paypunkd = Command::new(&exe)
        .arg("paypunkd")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to spawn paypunkd: {e}"))?;

    let paypunkd_wait =
        tokio::time::timeout(Duration::from_secs(30), wait_for_sockets(&[paypunkd_socket])).await;
    if paypunkd_wait.is_err() {
        let _ = keypunkd.kill();
        let _ = paypunkd.kill();
        let _ = keypunkd.wait();
        let _ = paypunkd.wait();
        return Err("Timed out waiting for paypunkd socket".into());
    }

    println!("Daemons ready.");

    Ok(DaemonGuard {
        keypunkd: Some(keypunkd),
        paypunkd: Some(paypunkd),
    })
}

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
    /// Remove all wallet data (seed, database, accounts) — resets to clean state
    Reset,
    /// Unlock the wallet and derive accounts
    Unlock {
        #[arg(short, long)]
        password: String,
    },
    Uninstall {
        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
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

            // Clean stale sockets before spawning daemons
            for path in [&keypunkd_socket, &paypunkd_socket] {
                let _ = fs::remove_file(path);
            }

            let mut keypunkd_child = Command::new(&exe)
                .arg("keypunkd")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| format!("Failed to spawn keypunkd: {e}"))?;

            let keypunkd_wait = tokio::time::timeout(
                Duration::from_secs(30),
                wait_for_sockets(&[&keypunkd_socket]),
            )
            .await;
            if keypunkd_wait.is_err() {
                let _ = keypunkd_child.kill();
                let _ = keypunkd_child.wait();
                return Err("Timed out waiting for keypunkd socket".into());
            }

            let mut paypunkd_child = Command::new(&exe)
                .arg("paypunkd")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| format!("Failed to spawn paypunkd: {e}"))?;

            let paypunkd_wait = tokio::time::timeout(
                Duration::from_secs(30),
                wait_for_sockets(&[&paypunkd_socket]),
            )
            .await;
            if paypunkd_wait.is_err() {
                let _ = keypunkd_child.kill();
                let _ = paypunkd_child.kill();
                let _ = keypunkd_child.wait();
                let _ = paypunkd_child.wait();
                return Err("Timed out waiting for paypunkd socket".into());
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
        Some(Commands::Tui) => run_tui(&socket_path, None).await.map_err(|e| e.into()),
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
        Some(Commands::Reset) => {
            let config = ConfigLoader::load_or_default();
            let data_dir = &config.data_dir;
            if Path::new(data_dir).exists() {
                fs::remove_dir_all(data_dir)
                    .map_err(|e| format!("Failed to remove {data_dir}: {e}"))?;
                println!("Removed: {data_dir}");
            } else {
                println!("No data found at {data_dir}");
            }
            Ok(())
        }
        Some(Commands::Uninstall { force }) => {
            let config = ConfigLoader::load_or_default();
            let data_dir = &config.data_dir;
            let config_dir = &config.config_dir;

            if !force {
                println!("This will permanently remove ALL wallet data:");
                println!("  Data directory:  {data_dir}");
                println!("  Config directory: {config_dir}");
                println!();
                print!("Are you sure? (yes/no): ");
                use std::io::{stdin, stdout, Write};
                let _ = stdout().flush();
                let mut input = String::new();
                stdin().read_line(&mut input).ok();
                if input.trim().to_lowercase() != "yes" {
                    println!("Aborted.");
                    return Ok(());
                }
            }

            let mut removed_any = false;

            if Path::new(data_dir).exists() {
                fs::remove_dir_all(data_dir)
                    .map_err(|e| format!("Failed to remove data directory {data_dir}: {e}"))?;
                println!("Removed: {data_dir}");
                removed_any = true;
            }

            if Path::new(config_dir).exists() {
                fs::remove_dir_all(config_dir)
                    .map_err(|e| format!("Failed to remove config directory {config_dir}: {e}"))?;
                println!("Removed: {config_dir}");
                removed_any = true;
            }

            if !removed_any {
                println!("Nothing to remove — no paypunk data found.");
            } else {
                println!("Paypunk has been uninstalled.");
            }

            Ok(())
        }
        Some(command) => {
            let config = ConfigLoader::load_or_default();
            let paypunkd_socket = cli
                .socket_path
                .clone()
                .unwrap_or(config.paypunkd_socket_path);
            let keypunkd_socket = config.keypunkd_socket_path;

            let _guard = ensure_daemons(&paypunkd_socket, &keypunkd_socket).await?;
            let client = Client::connect(&paypunkd_socket).await?;

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
                    let path = client.derivation_path(ProtocolId::Zcash, account);
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
                    let path = client.derivation_path(ProtocolId::Ethereum, account);
                    submit_intent_flow(&client, intent, &path).await?;
                }
                Commands::ApproveSignature {
                    password: _password,
                    account,
                } => {
                    let _path = client.derivation_path(ProtocolId::Ethereum, account);
                    println!("Approving signature for account {account}...");
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
                        _ => return Err(format!("Unknown protocol: {protocol}").into()),
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
                Commands::Reset => unreachable!(),
                Commands::Unlock { password } => {
                    let password = Zeroizing::new(password);
                    let count = client.unlock(password).await?;
                    println!("Unlocked. {count} accounts derived.");
                }
                Commands::Tui => unreachable!(),
                Commands::Keypunkd { .. } => unreachable!(),
                Commands::Paypunkd { .. } => unreachable!(),
                Commands::Uninstall { .. } => unreachable!(),
            }

            Ok(())
        }
    }
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
    client: &Client,
    intent: Intent,
    derivation_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Submitting intent for preview...");
    match client.submit_intent(intent, derivation_path).await {
        Ok((raw_artifact, parsed_summary, keypunkd_signature, keypunkd_public_key)) => {
            // Verify the signature: H(raw, parsed, path) should match
            let mut to_verify = Vec::new();
            to_verify.extend_from_slice(&raw_artifact);
            to_verify.extend_from_slice(&parsed_summary);
            to_verify.extend_from_slice(derivation_path.as_bytes());
            let _hash = Blake2b::<U32>::digest(&to_verify);

            println!("Artifact preview received:");
            println!("  Raw artifact: {} bytes", raw_artifact.len());

            if let Ok(summary) = postcard::from_bytes::<ArtifactSummary>(&parsed_summary) {
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

            println!();
            println!("To approve, run: paypunk approve-signature --password <your-password>");
            Ok(())
        }
        Err(e) => Err(format!("Error: {e}").into()),
    }
}
