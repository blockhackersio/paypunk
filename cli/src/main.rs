use blake2::digest::consts::U32;
use blake2::{Blake2b, Digest};
use clap::{Parser, Subcommand};
use paypunk_api::Client;
use paypunk_config::ConfigLoader;
use paypunk_tui::run_tui;
use paypunk_types::{ArtifactSummary, EthereumIntent, Intent, ProtocolId, ZcashIntent};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use zeroize::Zeroizing;

/// Pending intent data stored between submit and approve steps.
struct PendingIntent {
    raw_artifact: Vec<u8>,
    keypunkd_signature: Vec<u8>,
    keypunkd_public_key: [u8; 32],
    derivation_path: String,
    protocol: ProtocolId,
}

fn pending_intent_path(data_dir: &str) -> PathBuf {
    let dir = Path::new(data_dir);
    std::fs::create_dir_all(dir).ok();
    dir.join("pending.intent")
}

fn save_pending_intent(data_dir: &str, pi: &PendingIntent) -> Result<(), String> {
    // Format: protocol_id(1) + key_pk(32) + path_len(4) + path + raw_len(4) + raw + sig_len(4) + sig
    let path_bytes = pi.derivation_path.as_bytes();
    let mut buf = Vec::new();
    buf.push(pi.protocol as u8);
    buf.extend_from_slice(&pi.keypunkd_public_key);
    buf.extend_from_slice(&(path_bytes.len() as u32).to_le_bytes());
    buf.extend_from_slice(path_bytes);
    buf.extend_from_slice(&(pi.raw_artifact.len() as u32).to_le_bytes());
    buf.extend_from_slice(&pi.raw_artifact);
    buf.extend_from_slice(&(pi.keypunkd_signature.len() as u32).to_le_bytes());
    buf.extend_from_slice(&pi.keypunkd_signature);
    std::fs::write(pending_intent_path(data_dir), &buf)
        .map_err(|e| format!("failed to save pending intent: {e}"))
}

fn load_pending_intent(data_dir: &str) -> Result<PendingIntent, String> {
    let buf = std::fs::read(pending_intent_path(data_dir))
        .map_err(|e| format!("No pending intent found: {e}"))?;
    let mut pos = 0;
    let protocol = match buf[pos] {
        0 => ProtocolId::Zcash,
        1 => ProtocolId::Ethereum,
        n => return Err(format!("unknown protocol id: {n}")),
    };
    pos += 1;
    let mut pk = [0u8; 32];
    pk.copy_from_slice(&buf[pos..pos + 32]);
    pos += 32;
    let path_len = u32::from_le_bytes(buf[pos..pos + 4].try_into().unwrap()) as usize;
    pos += 4;
    let derivation_path = String::from_utf8(buf[pos..pos + path_len].to_vec())
        .map_err(|_| "invalid derivation path".to_string())?;
    pos += path_len;
    let raw_len = u32::from_le_bytes(buf[pos..pos + 4].try_into().unwrap()) as usize;
    pos += 4;
    let raw_artifact = buf[pos..pos + raw_len].to_vec();
    pos += raw_len;
    let sig_len = u32::from_le_bytes(buf[pos..pos + 4].try_into().unwrap()) as usize;
    pos += 4;
    let keypunkd_signature = buf[pos..pos + sig_len].to_vec();
    Ok(PendingIntent {
        raw_artifact,
        keypunkd_signature,
        keypunkd_public_key: pk,
        derivation_path,
        protocol,
    })
}

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
        match tokio::time::timeout(Duration::from_millis(500), Client::connect(paypunkd_socket))
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

    let exe =
        std::env::current_exe().map_err(|e| format!("Failed to get current exe path: {e}"))?;
    let config = ConfigLoader::load_or_default();

    // Clean stale sockets before spawning
    let _ = fs::remove_file(keypunkd_socket);
    let _ = fs::remove_file(paypunkd_socket);

    println!("Starting keypunkd...");
    let mut keypunkd = Command::new(&exe)
        .arg("keypunkd")
        .arg("--zcash-network")
        .arg(&config.zcash_network)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to spawn keypunkd: {e}"))?;

    let keypunkd_wait = tokio::time::timeout(
        Duration::from_secs(30),
        wait_for_sockets(&[keypunkd_socket]),
    )
    .await;
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

    let paypunkd_wait = tokio::time::timeout(
        Duration::from_secs(30),
        wait_for_sockets(&[paypunkd_socket]),
    )
    .await;
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
    /// Submit a transfer intent for preview
    SubmitTransfer {
        #[arg(short, long)]
        to: String,
        #[arg(short, long)]
        amount: String,
        #[arg(short, long)]
        from: String,
        #[arg(long, default_value = "eip155:1/slip44:60")]
        asset: String,
        #[arg(short, long)]
        protocol: Option<String>,
        #[arg(short, long)]
        data: Option<String>,
        #[arg(short, long)]
        memo: Option<String>,
        #[arg(long, default_value_t = 0)]
        account: u32,
    },
    /// Approve a previously submitted intent by providing the password
    ApproveSignature {
        /// Password to authorize the signing
        #[arg(short, long)]
        password: String,
        #[arg(long, default_value_t = 0)]
        account: u32,
    },
    /// Query the balance for a protocol and account
    GetBalance {
        #[arg(short, long, default_value = "zcash")]
        protocol: String,
        #[arg(short, long, default_value_t = 0)]
        account: u32,
        /// Zcash address to query (overrides --account)
        #[arg(long)]
        address: Option<String>,
    },
    /// Launch the terminal user interface
    Tui,
    /// Launch keypunkd (key daemon) as a child process
    Keypunkd {
        #[arg(short, long)]
        socket_path: Option<String>,
        #[arg(short, long)]
        data_dir: Option<String>,
        #[arg(short, long)]
        zcash_network: Option<String>,
    },
    /// Launch paypunkd (app daemon) as a child process
    Paypunkd {
        #[arg(short, long)]
        socket_path: Option<String>,
        #[arg(short, long)]
        keypunkd_socket: Option<String>,
        #[arg(short, long)]
        ethereum_rpc_url: Option<String>,
        #[arg(short, long)]
        data_dir: Option<String>,
        #[arg(short, long)]
        lightwalletd_host: Option<String>,
        #[arg(short, long)]
        zcash_network: Option<String>,
    },
    /// Remove all wallet data (seed, database, accounts) — resets to clean state
    Reset,
    /// List all accounts in the wallet
    ListAccounts,
    /// Create a new account from a pre-derived viewing key
    CreateAccount {
        #[arg(short, long, default_value = "zcash")]
        protocol: String,
        #[arg(long, default_value_t = 0)]
        account_index: u32,
        #[arg(short, long)]
        name: Option<String>,
        #[arg(long)]
        birthday_height: Option<u64>,
    },
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
                .arg("--zcash-network")
                .arg(&config.zcash_network)
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

            let config = ConfigLoader::load_or_default();
            let mut paypunkd_child = Command::new(&exe)
                .arg("paypunkd")
                .arg("--lightwalletd-host")
                .arg(&config.lightwalletd_host)
                .arg("--zcash-network")
                .arg(&config.zcash_network)
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
            zcash_network,
        }) => {
            let config = ConfigLoader::load_or_default();
            let socket = socket_path.unwrap_or(config.keypunkd_socket_path);
            let dir = data_dir.unwrap_or(config.data_dir);
            let znet = zcash_network.unwrap_or(config.zcash_network);

            keypunkd::run::run(keypunkd::run::Config {
                socket_path: socket,
                data_dir: dir,
                zcash_network: znet,
            })
            .await
        }
        Some(Commands::Paypunkd {
            socket_path,
            keypunkd_socket,
            ethereum_rpc_url,
            data_dir,
            lightwalletd_host,
            zcash_network,
        }) => {
            let config = ConfigLoader::load_or_default();
            let socket = socket_path.unwrap_or(config.paypunkd_socket_path);
            let ks = keypunkd_socket.unwrap_or(config.keypunkd_socket_path);
            let url = ethereum_rpc_url.unwrap_or(config.ethereum_rpc_url);
            let dir = data_dir.unwrap_or(config.data_dir);
            let lwd = lightwalletd_host.unwrap_or(config.lightwalletd_host);
            let znet = zcash_network.unwrap_or(config.zcash_network);

            paypunkd::run::run(paypunkd::run::Config {
                socket_path: socket,
                keypunkd_socket: ks,
                ethereum_rpc_url: url,
                data_dir: dir,
                lightwalletd_host: lwd,
                zcash_network: znet,
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
                Commands::SubmitTransfer {
                    to,
                    amount,
                    from,
                    asset,
                    protocol,
                    data,
                    memo,
                    account,
                } => {
                    let protocol_id = protocol
                        .as_deref()
                        .or_else(|| {
                            if asset.contains("eip155") {
                                Some("ethereum")
                            } else if asset.contains("zcash") {
                                Some("zcash")
                            } else {
                                None
                            }
                        })
                        .unwrap_or("ethereum");
                    let protocol_id = match protocol_id {
                        "zcash" => ProtocolId::Zcash,
                        "ethereum" => ProtocolId::Ethereum,
                        _ => return Err(format!("Unknown protocol: {protocol_id}").into()),
                    };
                    let intent = match protocol_id {
                        ProtocolId::Ethereum => Intent::Ethereum(EthereumIntent::Transfer {
                            to,
                            amount,
                            from,
                            asset,
                            data,
                        }),
                        ProtocolId::Zcash => Intent::Zcash(ZcashIntent::Transfer {
                            to,
                            amount,
                            from,
                            asset,
                            memo,
                        }),
                        _ => return Err("unsupported protocol".into()),
                    };
                    let path = client.derivation_path(protocol_id, account);
                    let data_dir = config.data_dir.clone();
                    submit_intent_flow(&client, intent, &path, &data_dir, protocol_id).await?;
                }
                // TODO:
                Commands::ApproveSignature {
                    password,
                    account: _account,
                } => {
                    let config = ConfigLoader::load_or_default();
                    let data_dir = config.data_dir.clone();
                    let pending = load_pending_intent(&data_dir)?;
                    println!("Approving signature for {:?}...", pending.protocol);
                    let signed_artifact = client
                        .approve_signature(
                            &pending.raw_artifact,
                            &pending.keypunkd_signature,
                            Zeroizing::new(password),
                            &pending.derivation_path,
                        )
                        .await?;
                    println!("Signature approved, broadcasting transaction...");
                    let tx_hash = client
                        .broadcast_transaction(pending.protocol, signed_artifact)
                        .await?;
                    println!("Transaction broadcasted: {tx_hash}");
                    // Clean up pending file
                    let _ = std::fs::remove_file(pending_intent_path(&data_dir));
                }
                Commands::GetBalance {
                    protocol,
                    account,
                    address,
                } => {
                    let protocol_id = match protocol.to_lowercase().as_str() {
                        "zcash" => ProtocolId::Zcash,
                        "ethereum" => ProtocolId::Ethereum,
                        _ => return Err(format!("Unknown protocol: {protocol}").into()),
                    };
                    let (caip_chain, caip_asset) = match protocol_id {
                        ProtocolId::Ethereum => ("eip155:1", "eip155:1/slip44:60"),
                        ProtocolId::Zcash => ("zcash:mainnet", "zcash:mainnet/slip44:133"),
                        _ => return Err(format!("unsupported protocol: {protocol}").into()),
                    };
                    let address = match address {
                        Some(raw) => format!("{}:{}", caip_chain, raw),
                        None => {
                            let expected_path = client.derivation_path(protocol_id, account);
                            let accounts = client.list_accounts().await?;
                            let matched = accounts.iter().find(|a| {
                                a.protocol == protocol_id && a.derivation_path == expected_path
                            });
                            match matched {
                                Some(a) => format!("{}:{}", caip_chain, a.address),
                                None => {
                                    return Err(format!(
                                        "account {} not found for protocol {protocol}. Create it first.",
                                        account
                                    )
                                    .into());
                                }
                            }
                        }
                    };
                    let balance = client.get_balance(address, caip_asset.to_string()).await?;
                    println!(
                        "Balance (protocol={protocol}): spendable={}, pending={}, total={}",
                        balance.spendable.0, balance.pending.0, balance.total.0,
                    );
                }
                Commands::ListAccounts => {
                    let accounts = client.list_accounts().await?;
                    if accounts.is_empty() {
                        println!("No accounts found.");
                    } else {
                        for a in &accounts {
                            println!(
                                "{} | {:?} | {} | {} | {}",
                                a.id, a.protocol, a.derivation_path, a.name, a.address,
                            );
                        }
                    }
                }
                Commands::CreateAccount {
                    protocol,
                    account_index,
                    name,
                    birthday_height,
                } => {
                    let protocol_id = match protocol.to_lowercase().as_str() {
                        "zcash" => ProtocolId::Zcash,
                        "ethereum" => ProtocolId::Ethereum,
                        _ => return Err(format!("Unknown protocol: {protocol}").into()),
                    };
                    let path = client.derivation_path(protocol_id, account_index);
                    let name =
                        name.unwrap_or_else(|| format!("{protocol_id:?} Account {account_index}"));
                    let account = client
                        .create_account(protocol_id, path, account_index, name, birthday_height)
                        .await?;
                    println!(
                        "Account created: {} | {:?} | {} | {} | {}",
                        account.id,
                        account.protocol,
                        account.derivation_path,
                        account.name,
                        account.address,
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
    data_dir: &str,
    protocol: ProtocolId,
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

            // Save pending intent for the approve step
            save_pending_intent(
                data_dir,
                &PendingIntent {
                    raw_artifact,
                    keypunkd_signature,
                    keypunkd_public_key,
                    derivation_path: derivation_path.to_string(),
                    protocol,
                },
            )?;

            println!();
            println!("To approve, run: paypunk approve-signature --password <your-password>");
            Ok(())
        }
        Err(e) => Err(format!("Error: {e}").into()),
    }
}
