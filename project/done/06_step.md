# Step 6: WalletDbActor Implementation

## Goal
Implement the `ProposeAndBuild` handler using `zcash_client_backend` APIs and add `Sync`/`GetStatus` message variants.

## Changes

### 1. `protocols/zcash/src/wallet_actor.rs`

Replace the entire file with the full implementation:

```rust
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use rand_core::OsRng;
use tactix::{Actor, Ctx, Handler, Message};
use zcash_client_backend::data_api::WalletRead;
use zcash_client_sqlite::util::SystemClock;
use zcash_client_sqlite::WalletDb;
use zcash_protocol::consensus::{BlockHeight, Network, Parameters};

use crate::lsp_client::LspClient;
use paypunk_types::SyncStatus;

/// Messages sent to the Zcash WalletDbActor.
#[derive(Debug, Message)]
#[response(Result<Vec<u8>, String>)]
pub enum WalletMessage {
    /// Build an unsigned PCZT for a transfer.
    ProposeAndBuild {
        public_key: Vec<u8>,
        account: u32,
        to: String,
        amount: u64,
        memo: Option<String>,
    },
    /// Trigger a chain sync from birthday height to latest.
    Sync {
        fvk: Vec<u8>,
        birthday_height: u64,
        lightwalletd_host: String,
    },
    /// Get the current sync status.
    GetStatus,
}

/// Tactix actor wrapping `zcash_client_sqlite::WalletDb` behind a Mutex.
pub struct WalletDbActor {
    pub db: Mutex<
        WalletDb<rusqlite::Connection, Network, SystemClock, OsRng>,
    >,
    pub params: Network,
    pub is_syncing: AtomicBool,
    pub current_height: AtomicU64,
    pub target_height: AtomicU64,
}

impl WalletDbActor {
    pub fn new(
        db: WalletDb<rusqlite::Connection, Network, SystemClock, OsRng>,
        params: Network,
    ) -> Self {
        Self {
            db: Mutex::new(db),
            params,
            is_syncing: AtomicBool::new(false),
            current_height: AtomicU64::new(0),
            target_height: AtomicU64::new(0),
        }
    }
}

impl Actor for WalletDbActor {}

impl Handler<WalletMessage> for WalletDbActor {
    async fn handle(&mut self, msg: WalletMessage, _ctx: &Ctx<Self>) -> Result<Vec<u8>, String> {
        match msg {
            WalletMessage::ProposeAndBuild {
                public_key,
                account: _account,
                to,
                amount,
                memo,
            } => {
                let db = self.db.lock().map_err(|e| e.to_string())?;

                // Parse the recipient address
                let to_addr = zcash_address::ZcashAddress::try_from_encoded(&to)
                    .map_err(|e| format!("invalid recipient address: {e}"))?;

                // Parse the amount from zatoshis
                let amount = zcash_protocol::value::Zatoshis::from_u64(amount)
                    .map_err(|_| "invalid amount".to_string())?;

                // Deserialize the FVK from bytes
                let fvk_bytes: [u8; 96] = public_key.as_slice().try_into()
                    .map_err(|_| "invalid FVK bytes: expected 96 bytes".to_string())?;
                let orchard_fvk = orchard::keys::FullViewingKey::from_bytes(&fvk_bytes)
                    .map_err(|e| format!("invalid FVK: {e}"))?;

                // Build the proposal
                let proposal = zcash_client_backend::propose_standard_transfer_to_address::<
                    _, OsRng, _
                >(
                    &*db,
                    &self.params,
                    OsRng,
                    to_addr,
                    amount,
                    memo.as_deref(),
                    None, // use default fee
                )
                .map_err(|e| format!("propose_transfer failed: {e}"))?;

                // Create PCZT from the proposal
                let pczt = zcash_client_backend::create_pczt_from_proposal::<
                    _, _, OsRng, _
                >(
                    &*db,
                    &self.params,
                    OsRng,
                    &proposal,
                )
                .map_err(|e| format!("create_pczt failed: {e}"))?;

                Ok(pczt.serialize())
            }
            WalletMessage::Sync {
                fvk: _fvk,
                birthday_height,
                lightwalletd_host,
            } => {
                if self.is_syncing.load(Ordering::SeqCst) {
                    return Err("sync already in progress".to_string());
                }

                self.is_syncing.store(true, Ordering::SeqCst);
                self.current_height.store(0, Ordering::SeqCst);

                // Connect to lightwalletd
                let lsp = LspClient::connect(&lightwalletd_host, self.params).await?;
                let latest = lsp.get_latest_height().await?;
                let latest_u64: u64 = latest.into();
                self.target_height.store(latest_u64, Ordering::SeqCst);

                let birthday = BlockHeight::from_u32(birthday_height as u32);

                // Scan blocks
                let db = self.db.lock().map_err(|e| e.to_string())?;
                let (scanned_from, scanned_to) = lsp.scan_range(&*db, birthday, latest).await?;
                drop(db);

                self.current_height.store(scanned_to, Ordering::SeqCst);
                self.is_syncing.store(false, Ordering::SeqCst);

                Ok(format!("synced from block {} to {}", scanned_from, scanned_to))
            }
            WalletMessage::GetStatus => {
                let status = SyncStatus {
                    is_syncing: self.is_syncing.load(Ordering::SeqCst),
                    current_height: self.current_height.load(Ordering::SeqCst),
                    target_height: self.target_height.load(Ordering::SeqCst),
                };
                postcard::to_allocvec(&status)
                    .map_err(|e| format!("serialize status failed: {e}"))
            }
        }
    }
}
```

### 2. `protocols/zcash/src/wallet_client.rs`

Add methods:
```rust
/// Trigger a sync for the given account.
pub async fn sync(
    &self,
    fvk: Vec<u8>,
    birthday_height: u64,
    lightwalletd_host: String,
) -> Result<String, String> {
    self.recipient
        .ask(WalletMessage::Sync { fvk, birthday_height, lightwalletd_host })
        .await
}

/// Get the current sync status.
pub async fn get_status(&self) -> Result<SyncStatus, String> {
    let bytes = self.recipient
        .ask(WalletMessage::GetStatus)
        .await?;
    postcard::from_bytes(&bytes)
        .map_err(|e| format!("deserialize status failed: {e}"))
}
```

Add `use paypunk_types::SyncStatus;` to imports.

### 3. `protocols/zcash/Cargo.toml`

Ensure the `wallet` feature includes all needed deps:
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

## Verification
- `cargo build -p paypunk-chains-zcash` succeeds
