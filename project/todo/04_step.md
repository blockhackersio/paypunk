# Step 4: TUI API Layer

## Goal
Add `SyncStatus` type and `sync()`/`get_sync_status()` methods to the TUI API trait, mock, and real implementations.

## Changes

### 1. `tui/src/api/types.rs`

Add after `AddressBookData`:
```rust
#[derive(Debug, Clone, Default)]
pub struct SyncStatus {
    pub is_syncing: bool,
    pub current_height: u64,
    pub target_height: u64,
}
```

### 2. `tui/src/api/mod.rs`

Add to `WalletApi` trait:
```rust
async fn sync(&self, protocol: &str) -> Result<(), ApiError>;
async fn get_sync_status(&self, protocol: &str) -> SyncStatus;
```

### 3. `tui/src/api/mock.rs`

Add to `MockWalletApi`:
```rust
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
```

Add fields to `MockData`:
```rust
sync_in_progress: bool,
sync_current: u64,
sync_target: u64,
```

Initialize in `MockData` constructor:
```rust
sync_in_progress: false,
sync_current: 0,
sync_target: 0,
```

Implement methods:
```rust
async fn sync(&self, _protocol: &str) -> Result<(), ApiError> {
    // Mock: pretend sync completed instantly
    Ok(())
}

async fn get_sync_status(&self, _protocol: &str) -> SyncStatus {
    SyncStatus {
        is_syncing: false,
        current_height: 2800000,
        target_height: 2800000,
    }
}
```

### 4. `tui/src/api/real.rs`

Implement methods:
```rust
async fn sync(&self, protocol: &str) -> Result<(), ApiError> {
    let protocol_id = match protocol {
        "Zcash" => paypunk_types::ProtocolId::Zcash,
        "Ethereum" => paypunk_types::ProtocolId::Ethereum,
        _ => return Err(ApiError(format!("unknown protocol: {protocol}"))),
    };
    self.client.sync(protocol_id).await.map_err(ApiError)
}

async fn get_sync_status(&self, protocol: &str) -> SyncStatus {
    let protocol_id = match protocol {
        "Zcash" => paypunk_types::ProtocolId::Zcash,
        "Ethereum" => paypunk_types::ProtocolId::Ethereum,
        _ => return SyncStatus::default(),
    };
    match self.client.get_sync_status(protocol_id).await {
        Ok(s) => SyncStatus {
            is_syncing: s.is_syncing,
            current_height: s.current_height,
            target_height: s.target_height,
        },
        Err(_) => SyncStatus::default(),
    }
}
```

Add `use crate::api::types::SyncStatus;` to imports.

## Verification
- `cargo build -p paypunk-tui` succeeds (may have warnings about unused methods)
