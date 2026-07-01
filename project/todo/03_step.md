# Step 3: API Crate

## Goal
Add `sync()` and `get_sync_status()` methods to the `Client` and `PaypunkService`.

## Changes

### 1. `paypunkd/src/services.rs`

Add methods to `PaypunkService`:
```rust
pub async fn sync(&self, protocol: ProtocolId) -> Result<(), String> {
    let request = PaypunkdRequest::Sync { protocol };
    let response = self.recipient.ask(IpcMessage::new(&request)).await?;
    let decoded: PaypunkdResponse = postcard::from_bytes(&response)
        .map_err(|e| format!("deserialize error: {e}"))?;
    match decoded {
        PaypunkdResponse::SyncAck => Ok(()),
        PaypunkdResponse::Error { message } => Err(message),
        _ => Err("unexpected response".to_string()),
    }
}

pub async fn get_sync_status(&self, protocol: ProtocolId) -> Result<SyncStatus, String> {
    let request = PaypunkdRequest::GetSyncStatus { protocol };
    let response = self.recipient.ask(IpcMessage::new(&request)).await?;
    let decoded: PaypunkdResponse = postcard::from_bytes(&response)
        .map_err(|e| format!("deserialize error: {e}"))?;
    match decoded {
        PaypunkdResponse::SyncStatusResult { status } => Ok(status),
        PaypunkdResponse::Error { message } => Err(message),
        _ => Err("unexpected response".to_string()),
    }
}
```

Add `use paypunk_types::SyncStatus;` to imports.

### 2. `api/src/client.rs`

Add methods to `Client`:
```rust
pub async fn sync(&self, protocol: ProtocolId) -> Result<(), String> {
    self.service.sync(protocol).await
}

pub async fn get_sync_status(&self, protocol: ProtocolId) -> Result<SyncStatus, String> {
    self.service.get_sync_status(protocol).await
}
```

Add `use paypunk_types::SyncStatus;` to imports.

### 3. `api/src/functions.rs`

No changes needed — these are thin wrappers that delegate to the service.

## Verification
- `cargo build -p paypunk-api` succeeds
- `cargo build -p paypunkd` succeeds
