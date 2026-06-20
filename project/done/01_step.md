# Step 1: Extract daemon run() functions and remove standalone binary targets

## Context

The `keypunkd` and `paypunkd` crates currently produce standalone binaries (via `[[bin]]` in their Cargo.toml) with `main.rs` files that parse CLI args, init tracing, and run the daemon. We want these crates to be library-only, exposing a `run()` function that the CLI can call directly (in-process) or via child process spawning.

## Changes

### 1. `keypunkd/src/run.rs` (new file)

Create a public `run` module with a `Config` struct and an async `run()` function that encapsulates the current `main.rs` logic:

```rust
use paypunk_ipc::IpcReceiver;
use paypunk_types::ProtocolId;
use tactix::Actor;
use tracing::info;
use tracing_subscriber::EnvFilter;

pub struct Config {
    pub socket_path: String,
    pub data_dir: String,
}

pub async fn run(config: Config) -> Result<(), Box<dyn std::error::Error>> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .try_init();

    let keystore = keypunkd::crypto::Keypair::new();
    let (secret, public) = keystore.keypair();
    let seed_store = keypunkd::seed_store::FilesystemSeedStore::new(
        std::path::PathBuf::from(&config.data_dir)
            .join("seed.enc")
            .into_boxed_path(),
    );

    let mut protocols = keypunkd::protocol::ProtocolService::new();
    protocols.register(
        ProtocolId::Zcash,
        Box::new(paypunk_chains_zcash::protocol::ZcashProtocol {
            params: zcash_protocol::consensus::Network::MainNetwork,
        }),
    );
    protocols.register(
        ProtocolId::Ethereum,
        Box::new(paypunk_chains_ethereum::protocol::EthereumProtocol::new(())),
    );
    info!("registered protocols: Zcash, Ethereum");

    let keypunkd = keypunkd::Keypunkd::new(keystore, seed_store, protocols).start();

    let server = IpcReceiver::bind_with(&config.socket_path, secret, public).await?;
    info!("keypunkd listening on {}", config.socket_path);

    let serve = tokio::spawn(async move {
        if let Err(e) = server.serve(keypunkd).await {
            tracing::error!(error = %e, "server error");
        }
    });

    tokio::signal::ctrl_c().await?;
    info!("shutting down");
    serve.abort();
    Ok(())
}
```

Note: uses `try_init()` instead of `init()` so tracing is not double-initialized when called in-process from the CLI.

### 2. `keypunkd/src/lib.rs`

Add `pub mod run;`

### 3. `keypunkd/Cargo.toml`

Remove the `[[bin]]` section:
```toml
# DELETE these 3 lines:
# [[bin]]
# name = "keypunkd"
# path = "src/main.rs"
```

### 4. `keypunkd/src/main.rs` — DELETE this file

### 5. `paypunkd/src/run.rs` (new file)

Create a public `run` module:

```rust
use keypunkd::crypto::Keypair;
use paypunk_ipc::{IpcReceiver, IpcSender};
use paypunkd::config::{ConfigSource, TomlConfig};
use paypunkd::database::Database;
use paypunkd::protocol_service::ProtocolService;
use paypunkd::Paypunkd;
use tactix::{Actor, Sender};
use tracing::info;
use tracing_subscriber::EnvFilter;

pub struct Config {
    pub socket_path: String,
    pub keypunkd_socket: String,
    pub rpc_url: String,
    pub data_dir: String,
}

pub async fn run(config: Config) -> Result<(), Box<dyn std::error::Error>> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .try_init();

    let keystore = Keypair::new();
    let (secret, public) = keystore.keypair();

    info!("connecting to keypunkd");
    let keypunkd = IpcSender::connect(&config.keypunkd_socket).await?;
    let recipient = keypunkd.recipient();

    let zcash = paypunk_chains_zcash::protocol::ZcashProtocol {
        params: zcash_protocol::consensus::Network::MainNetwork,
    };
    let eth_client =
        paypunk_chains_ethereum::rpc::HttpRpcClient::new(config.rpc_url.clone());
    let ethereum = paypunk_chains_ethereum::protocol::EthereumProtocol::new(eth_client);
    let mut protocols = ProtocolService::new();
    protocols.register(Box::new(zcash));
    protocols.register(Box::new(ethereum));
    info!("registered protocols: Zcash, Ethereum");

    let db = Database::open(std::path::Path::new(&config.data_dir))
        .map_err(|e| format!("failed to open database: {e}"))?;
    info!("database opened");

    let paypunkd = Paypunkd::new(recipient, protocols, db, keystore).start();

    let server = IpcReceiver::bind_with(&config.socket_path, secret, public).await?;
    info!("paypunkd listening on {}", config.socket_path);

    let serve = tokio::spawn(async move {
        if let Err(e) = server.serve(paypunkd).await {
            tracing::error!(error = %e, "server error");
        }
    });

    tokio::signal::ctrl_c().await?;
    info!("shutting down");
    serve.abort();
    Ok(())
}
```

### 6. `paypunkd/src/lib.rs`

Add `pub mod run;`

### 7. `paypunkd/Cargo.toml`

Remove the `[[bin]]` section:
```toml
# DELETE these 3 lines:
# [[bin]]
# name = "paypunkd"
# path = "src/main.rs"
```

### 8. `paypunkd/src/main.rs` — DELETE this file

### 9. Check `paypunkd/Cargo.toml` dependencies

The `paypunkd` crate currently depends on `clap` (for its own main.rs CLI parsing). After removing main.rs, check if `clap` is still needed by any lib module. If not, remove it from paypunkd's `[dependencies]`.

Similarly check if `tracing-subscriber` is already in paypunkd's deps (it should be — it's used in the run function).

## Verification

- `cargo build` succeeds (no orphaned main.rs, no missing deps)
- `cargo test` passes (integration tests wire actors in-process, don't use binary entry points)
- The `keypunkd` and `paypunkd` library crates compile and expose `run::Config` and `run::run()`

## Acceptance criteria

- No standalone `keypunkd` or `paypunkd` binaries exist
- Both crates compile as library-only crates
- `keypunkd::run::run(config)` starts the key daemon
- `paypunkd::run::run(config)` starts the app daemon
- All existing tests pass
