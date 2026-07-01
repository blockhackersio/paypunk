# Step 12: CLI Flags

## Goal
Add `--lightwalletd-host` and `--zcash-network` flags to the paypunkd subcommand in the CLI.

## Changes

### `cli/src/main.rs`

Add flags to the `Paypunkd` subcommand:
```rust
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
```

In the handler (around line 305-324):
```rust
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
```

Also update the auto-spawn in the `None` command handler (around line 254-259):
```rust
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
```

## Verification
- `cargo build -p paypunk` succeeds
- `cargo build` (workspace) succeeds
