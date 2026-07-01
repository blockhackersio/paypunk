# Step 5: LspClient Module

## Goal
Create the lightwalletd gRPC client module for chain scanning and transaction broadcast.

## Changes

### 0. Workspace `Cargo.toml`

Move `tonic` from dev-dependencies to regular dependencies (line 100 → after line 68):
```toml
# gRPC
tonic = "0.14"
```

### 1. `protocols/zcash/Cargo.toml`

Move `tonic` and `zcash_client_backend` (with lightwalletd features) from dev-dependencies to regular dependencies:

```toml
# Add to [dependencies]:
tonic = { workspace = true, features = ["tls", "tls-roots"] }
zcash_client_backend = { workspace = true, default-features = false, features = [
    "orchard", "lightwalletd-tonic", "sync", "transparent-inputs", "zcash_proofs"
] }
```

Keep the existing `wallet` feature optional deps as they are. The `wallet` feature should now also include `tonic` and the full `zcash_client_backend` feature set:

```toml
wallet = [
    "dep:zcash_client_backend",
    "dep:zcash_client_sqlite",
    "dep:tactix",
    "dep:tokio",
    "dep:rusqlite",
    "dep:secrecy",
    "dep:tonic",
]
```

### 2. `protocols/zcash/src/lsp_client.rs` (NEW FILE)

```rust
use zcash_client_backend::lightwalletd::LightWalletdClient;
use zcash_client_backend::lightwalletd::tonic_rpc::TonicRpcClient;
use zcash_client_sqlite::WalletDb;
use zcash_protocol::consensus::{BlockHeight, Network, Parameters};
use rand_core::OsRng;
use zcash_client_sqlite::util::SystemClock;

/// Lightwalletd gRPC client for Zcash chain interaction.
pub struct LspClient {
    inner: TonicRpcClient,
    params: Network,
}

impl LspClient {
    /// Connect to a lightwalletd endpoint.
    pub async fn connect(host: &str, params: Network) -> Result<Self, String> {
        let inner = TonicRpcClient::new(host)
            .map_err(|e| format!("failed to create lightwalletd client: {e}"))?;
        Ok(Self { inner, params })
    }

    /// Get the latest block height from lightwalletd.
    pub async fn get_latest_height(&self) -> Result<BlockHeight, String> {
        let info = self.inner.get_info().await
            .map_err(|e| format!("lightwalletd get_info failed: {e}"))?;
        let height = info.block_height;
        Ok(BlockHeight::from_u32(height))
    }

    /// Broadcast a raw transaction to the network.
    pub async fn broadcast_tx(&self, tx_bytes: &[u8]) -> Result<String, String> {
        let tx_hash = self.inner.broadcast_transaction(tx_bytes).await
            .map_err(|e| format!("broadcast failed: {e}"))?;
        Ok(hex::encode(tx_hash))
    }

    /// Scan a range of blocks into the WalletDb.
    /// Returns (scanned_from, scanned_to) heights.
    pub async fn scan_range(
        &self,
        wallet_db: &WalletDb<rusqlite::Connection, Network, SystemClock, OsRng>,
        from_height: BlockHeight,
        to_height: BlockHeight,
    ) -> Result<(u64, u64), String> {
        use zcash_client_backend::lightwalletd::LightWalletdClient as _;

        let from: u64 = from_height.into();
        let to: u64 = to_height.into();

        let mut current = from;
        let batch_size = 100; // scan 100 blocks at a time

        while current < to {
            let batch_end = (current + batch_size).min(to);
            let start = BlockHeight::from_u32(current as u32);
            let end = BlockHeight::from_u32(batch_end as u32);

            // Get blocks from lightwalletd
            let blocks = self.inner.get_block_range(start, end).await
                .map_err(|e| format!("get_block_range failed: {e}"))?;

            // Scan each block into the WalletDb
            for block in blocks {
                // The scan_block function is in zcash_client_backend
                // This is a simplified call — actual API may vary
                let _ = wallet_db;
                let _ = block;
                // TODO: Call zcash_client_backend::scan_block() for each block
                // This requires the FVK to be registered in the WalletDb
                // which is done via zcash_client_backend::put_notes_and_metadata
            }

            current = batch_end;
        }

        Ok((from, current))
    }
}
```

**Note**: The actual `scan_range` implementation will need to use `zcash_client_backend::scanning::scan_block` or similar. The exact API depends on the zcash_client_backend version. This step creates the module structure — the scanning logic will be refined in Step 6 when WalletDbActor uses it.

### 3. `protocols/zcash/src/lib.rs`

Add the module:
```rust
pub mod lsp_client;
```

Remove `#[cfg(feature = "wallet")]` gates from `wallet_actor` and `wallet_client` modules (make them always compiled):
```rust
pub mod wallet_actor;
pub mod wallet_client;
```

## Verification
- `cargo build -p paypunk-chains-zcash` succeeds
