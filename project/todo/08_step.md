# Step 8: paypunkd Wiring

## Goal
Wire everything together in paypunkd: update config, create WalletDb, start WalletDbActor, inject into ZcashProtocol, handle sync IPC messages.

## Changes

### 1. `paypunkd/src/config.rs`

Add to `ConfigSource` trait:
```rust
fn lightwalletd_host(&self) -> &str;
fn zcash_network(&self) -> &str;
```

Implement in `HardcodedConfig`:
```rust
fn lightwalletd_host(&self) -> &str {
    "" // not configured by default
}
fn zcash_network(&self) -> &str {
    "testnet"
}
```

Implement in `TomlConfig`:
```rust
fn lightwalletd_host(&self) -> &str {
    &self.config.lightwalletd_host
}
fn zcash_network(&self) -> &str {
    &self.config.zcash_network
}
```

### 2. `paypunkd/src/run.rs`

Update `Config` struct:
```rust
pub struct Config {
    pub socket_path: String,
    pub keypunkd_socket: String,
    pub ethereum_rpc_url: String,
    pub data_dir: String,
    pub lightwalletd_host: String,
    pub zcash_network: String,
}
```

Update `run()` to create WalletDb and inject into ZcashProtocol:

```rust
pub async fn run(config: Config) -> Result<(), Box<dyn std::error::Error>> {
    // ... existing setup ...

    // Determine Zcash network
    let zcash_params = match config.zcash_network.to_lowercase().as_str() {
        "mainnet" => zcash_protocol::consensus::Network::MainNetwork,
        "testnet" => zcash_protocol::consensus::Network::TestNetwork,
        _ => {
            tracing::warn!("unknown zcash network '{}', defaulting to testnet", config.zcash_network);
            zcash_protocol::consensus::Network::TestNetwork
        }
    };

    // Create Zcash WalletDb
    let zcash_db_dir = std::path::Path::new(&config.data_dir)
        .join("zcash")
        .join(&config.zcash_network);
    std::fs::create_dir_all(&zcash_db_dir)
        .map_err(|e| format!("failed to create zcash db dir: {e}"))?;
    let zcash_db_path = zcash_db_dir.join("wallet.db");

    let zcash_conn = rusqlite::Connection::open(&zcash_db_path)
        .map_err(|e| format!("failed to open zcash wallet db: {e}"))?;
    let wallet_db = zcash_client_sqlite::WalletDb::new(
        zcash_conn,
        zcash_params,
        zcash_client_sqlite::util::SystemClock,
        rand_core::OsRng,
    );

    let wallet_actor = paypunk_chains_zcash::wallet_actor::WalletDbActor::new(
        wallet_db, zcash_params
    ).start();
    let wallet_recipient = wallet_actor.recipient();

    let zcash_wallet_client = paypunk_chains_zcash::wallet_client::ZcashWalletClient {
        recipient: wallet_recipient,
    };

    let zcash = paypunk_chains_zcash::protocol::ZcashProtocol {
        params: zcash_params,
        wallet_client: Some(zcash_wallet_client),
        lightwalletd_host: Some(config.lightwalletd_host.clone()),
    };

    // ... rest of existing setup ...
}
```

Add needed imports to `run.rs`:
```rust
use paypunk_chains_zcash::wallet_actor::WalletDbActor;
use paypunk_chains_zcash::wallet_client::ZcashWalletClient;
```

### 3. `paypunkd/src/paypunkd.rs`

Add `Sync` and `GetSyncStatus` handler methods:

```rust
async fn sync(&self, protocol: ProtocolId) -> PaypunkdResponse {
    info!(?protocol, "handling Sync");
    self.respond(
        "sync",
        usecases::sync(&self.protocols, protocol).await,
        |()| PaypunkdResponse::SyncAck,
    )
}

async fn get_sync_status(&self, protocol: ProtocolId) -> PaypunkdResponse {
    info!(?protocol, "handling GetSyncStatus");
    self.respond(
        "get_sync_status",
        usecases::get_sync_status(&self.protocols, protocol).await,
        |status| PaypunkdResponse::SyncStatusResult { status },
    )
}
```

Add match arms in the `Handler<IpcMessage>` impl:
```rust
PaypunkdRequest::Sync { protocol } => self.sync(protocol).await,
PaypunkdRequest::GetSyncStatus { protocol } => self.get_sync_status(protocol).await,
```

### 4. `paypunkd/src/usecases.rs`

Implement `sync()` and `get_sync_status()`:

```rust
/// Trigger a chain sync for the given protocol.
pub async fn sync(
    protocols: &ProtocolService,
    protocol: ProtocolId,
) -> Result<(), String> {
    match protocol {
        ProtocolId::Zcash => {
            // For Zcash, sync is handled by the WalletDbActor
            // The protocol's build() and get_balance() methods
            // will check sync status internally
            // For now, sync is a no-op that returns success
            // The actual sync is triggered by WalletDbActor::Sync
            info!("sync requested for Zcash");
            Ok(())
        }
        _ => Err(format!("sync not supported for {protocol:?}")),
    }
}

/// Get the current sync status for the given protocol.
pub async fn get_sync_status(
    protocols: &ProtocolService,
    protocol: ProtocolId,
) -> Result<SyncStatus, String> {
    match protocol {
        ProtocolId::Zcash => {
            // Return a default "not syncing" status for now
            // The actual status comes from WalletDbActor
            Ok(SyncStatus {
                is_syncing: false,
                current_height: 0,
                target_height: 0,
            })
        }
        _ => Err(format!("sync status not supported for {protocol:?}")),
    }
}
```

Add `use paypunk_types::SyncStatus;` to imports.

### 5. `paypunkd/Cargo.toml`

Add dependencies:
```toml
zcash_client_sqlite = { workspace = true, features = ["orchard", "transparent-inputs"] }
rusqlite = { workspace = true, features = ["bundled"] }  # already present
```

## Verification
- `cargo build -p paypunkd` succeeds
