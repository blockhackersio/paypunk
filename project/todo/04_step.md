# Step 04: Add IPC messages for unlock and wallet-existence flow

## Goal

Add all new IPC message types and handler stubs needed for the unlock flow: wallet-existence check, encryption key retrieval, bulk viewing key derivation, and database unlock.

## Tasks

### 4a. Keypunkd messages (`keypunkd/src/messages.rs`)

Add variants to `KeypunkdRequest`:
```rust
HasSeed,
BulkExportViewingKeys {
    encrypted_password: Vec<u8>,
    client_public_key: [u8; 32],
    protocols: Vec<ProtocolId>,
    start_account: u32,
    count: u32,
}
```

Add variants to `KeypunkdResponse`:
```rust
HasSeed { exists: bool },
ViewingKeys { keys: Vec<(ProtocolId, u32, Vec<u8>)> },
```

### 4b. Keypunkd handlers (`keypunkd/src/keypunkd.rs`)

Add handlers:
- `has_seed()` — checks if `seed_store.read()` returns `Some`
- `bulk_export_viewing_keys()` — iterates `start_account..start_account+count` for each protocol, calls `export_viewing_key` for each, collects results

### 4c. Keypunkd usecases (`keypunkd/src/usecases.rs`)

Add `bulk_export_viewing_keys()` function that loops over protocols and account indices, calling `export_viewing_key()` for each.

### 4d. Keypunkd services (`keypunkd/src/services.rs`)

Add methods:
- `has_seed() -> Result<bool, String>`
- `bulk_export_viewing_keys(...) -> Result<Vec<(ProtocolId, u32, Vec<u8>)>, String>`

### 4e. Paypunkd messages (`paypunkd/src/messages.rs`)

Add variants to `PaypunkdRequest`:
```rust
GetPaypunkdEncryptionKey,
HasSeed,
Unlock {
    encrypted_db_password: Vec<u8>,        // encrypted to paypunkd's public key
    ephemeral_public_key: [u8; 32],
    encrypted_keypunkd_password: Vec<u8>,   // encrypted to keypunkd's public key
    keypunkd_client_pk: [u8; 32],
}
BulkDeriveAccounts {
    encrypted_password: Vec<u8>,
    client_public_key: [u8; 32],
    count: u32,
}
```

Add variants to `PaypunkdResponse`:
```rust
PaypunkdEncryptionKey { key: [u8; 32] },
HasSeed { exists: bool },
UnlockSuccess { accounts_count: u32 },
AccountsBulkDerived { accounts: Vec<Account> },
```

### 4f. Paypunkd handlers (`paypunkd/src/paypunkd.rs`)

Add handler stubs (full implementation in Step 05):
- `get_paypunkd_encryption_key()` — returns paypunkd's own X25519 public key
- `has_seed()` — forwards to keypunkd, returns result
- `unlock()` — stub, returns error "not implemented"
- `bulk_derive_accounts()` — stub, returns error "not implemented"

### 4g. Paypunkd main.rs

- Paypunkd creates its own `Keypair` (already does this in `main.rs`)
- Store the keypair on `Paypunkd` actor so it can decrypt messages
- Add `paypunkd_public_key` field to `Paypunkd` struct

### 4h. Paypunkd services (`paypunkd/src/services.rs`)

Add methods:
- `get_paypunkd_encryption_key() -> Result<[u8; 32], String>`
- `has_seed() -> Result<bool, String>`
- `unlock(...) -> Result<u32, String>`
- `bulk_derive_accounts(...) -> Result<Vec<Account>, String>`

## Acceptance Criteria

- [ ] `cargo check` succeeds for whole workspace
- [ ] `cargo test` passes for all crates
- [ ] All new message types serialize/deserialize correctly (postcard round-trip)
- [ ] `keypunkd::messages::KeypunkdRequest::HasSeed` is a unit variant (no payload)
- [ ] `Paypunkd` actor holds its own `Keypair` for decryption
- [ ] Handler stubs compile and return appropriate error responses
- [ ] Code is committed with message: "feat: add IPC messages for unlock and wallet-existence flow"
