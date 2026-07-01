# Step 2: IPC Messages

## Goal
Extend `PaypunkdRequest` and `PaypunkdResponse` with sync-related messages and birthday height support for account creation.

## Changes

### 1. `paypunkd/src/messages.rs`

Add `SyncStatus` to the imports:
```rust
use paypunk_types::{Account, Balance, Intent, ProtocolId, ProtocolMetadata, SyncStatus};
```

Add these variants to `PaypunkdRequest`:
```rust
// Trigger a chain sync for the given protocol
Sync {
    protocol: ProtocolId,
},
// Poll sync status for the given protocol
GetSyncStatus {
    protocol: ProtocolId,
},
```

Extend `CreateAccount` with optional birthday_height:
```rust
CreateAccount {
    protocol: ProtocolId,
    derivation_path: String,
    account_index: u32,
    name: String,
    birthday_height: Option<u64>,
},
```

Add these variants to `PaypunkdResponse`:
```rust
SyncAck,
SyncStatusResult {
    status: SyncStatus,
},
```

### 2. Update all existing `CreateAccount` callers to pass `birthday_height: None`

#### `paypunkd/src/paypunkd.rs` (line 470-478)
Change the match arm to extract and pass the new field:
```rust
PaypunkdRequest::CreateAccount {
    protocol,
    derivation_path,
    account_index,
    name,
    birthday_height,
} => {
    self.create_account(protocol, derivation_path, account_index, name, birthday_height)
        .await
}
```

Update `create_account` method signature (line 212-234):
```rust
async fn create_account(
    &self,
    protocol: ProtocolId,
    derivation_path: String,
    account_index: u32,
    name: String,
    birthday_height: Option<u64>,
) -> PaypunkdResponse {
    info!(?protocol, account_index, name, ?birthday_height, "creating account");
    self.respond(
        "create_account",
        usecases::create_account(
            &self.db,
            &self.protocols,
            self.accounts_repo.as_ref(),
            protocol,
            derivation_path,
            account_index,
            name,
            birthday_height,
        )
        .await,
        |account| PaypunkdResponse::AccountCreated { account },
    )
}
```

#### `paypunkd/src/paypunkd.rs` (line 360-369, inside `unlock`)
Update the `create_account` call to pass `birthday_height: None`:
```rust
let _ = usecases::create_account(
    &self.db,
    &self.protocols,
    self.accounts_repo.as_ref(),
    pid,
    path,
    account_index,
    name,
    None, // birthday_height — default for auto-created accounts
)
.await;
```

#### `api/src/client.rs` (line 118-133)
Update `create_account` method signature:
```rust
pub async fn create_account(
    &self,
    protocol: ProtocolId,
    derivation_path: String,
    account_index: u32,
    name: String,
    birthday_height: Option<u64>,
) -> Result<Account, String> {
    crate::functions::create_account(
        &self.service,
        protocol,
        derivation_path,
        account_index,
        name,
        birthday_height,
    )
    .await
}
```

#### `api/src/functions.rs` (line 222-233)
Update `create_account` function:
```rust
pub async fn create_account(
    service: &paypunkd::services::PaypunkService,
    protocol: ProtocolId,
    derivation_path: String,
    account_index: u32,
    name: String,
    birthday_height: Option<u64>,
) -> Result<Account, String> {
    service
        .create_account(protocol, derivation_path, account_index, name, birthday_height)
        .await
}
```

#### `paypunkd/src/services.rs`
Add `create_account` method with birthday_height:
```rust
pub async fn create_account(
    &self,
    protocol: ProtocolId,
    derivation_path: String,
    account_index: u32,
    name: String,
    birthday_height: Option<u64>,
) -> Result<Account, String> {
    let request = PaypunkdRequest::CreateAccount {
        protocol,
        derivation_path,
        account_index,
        name,
        birthday_height,
    };
    let response = self.recipient.ask(IpcMessage::new(&request)).await?;
    let decoded: PaypunkdResponse = postcard::from_bytes(&response)
        .map_err(|e| format!("deserialize error: {e}"))?;
    match decoded {
        PaypunkdResponse::AccountCreated { account } => Ok(account),
        PaypunkdResponse::Error { message } => Err(message),
        _ => Err("unexpected response".to_string()),
    }
}
```

## Verification
- `cargo build -p paypunkd` succeeds
- `cargo build -p paypunk-api` succeeds

