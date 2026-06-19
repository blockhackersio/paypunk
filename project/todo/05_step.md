# Step 05: Implement database locked/unlock and remove hardcoded password

**Prerequisites**: Step 04 (IPC message types exist, handler stubs compile)

## Goal

Implement the core unlock flow: paypunkd starts with the DB in locked state, accepts an `Unlock` message with an encrypted password, decrypts the DB, and becomes operational. Remove the hardcoded `"paypunk-default-password"` entirely. On unlock, if no accounts exist, bulk-derive viewing keys from keypunkd.

## Key files

- `paypunkd/src/database/db.rs:21-101` — `Database` struct, `open()`, `close()`
- `paypunkd/src/database/encryption.rs:1-66` — `encrypt_db()`, `decrypt_db()`, `DB_SALT`
- `paypunkd/src/database/encryption.rs:17` — `DB_SALT: &[u8] = b"paypunk-db-v1"` (remove this)
- `paypunkd/src/paypunkd.rs:12-238` — `Paypunkd` actor + handlers
- `paypunkd/src/main.rs:71-72` — where `Database::open()` is called with hardcoded password
- `paypunkd/src/usecases.rs:141-183` — `create_account()` usecase
- `tests/tests/integration_test.rs:117` — `TestBuilder::build()` calls `Database::open()`

## Tasks

### 5a. Database changes (`paypunkd/src/database/db.rs`)

- `Database::open(data_dir)` — no password parameter. If encrypted DB exists, open in **locked** state (store the encrypted bytes in memory, don't decrypt yet). If not, create a placeholder.
- Add `Database::unlock(password: &str) -> Result<()>`:
  - Decrypts the stored encrypted bytes with the password
  - Writes plaintext to temp file
  - Opens SQLite connection
  - Runs migrations
- Add `Database::wallet_exists() -> bool` — checks if encrypted DB file exists on disk
- Add `Database::is_locked() -> bool` — returns whether DB is still locked (no conn yet)
- Update `Database::close()` — only re-encrypt if DB was unlocked (has conn)

### 5b. Encryption changes (`paypunkd/src/database/encryption.rs`)

- Remove the hardcoded `DB_SALT` constant (`encryption.rs:17`)
- `derive_db_key()` should use a random salt (stored alongside the ciphertext, like keypunkd's `key.rs` does)
- The encrypted blob format becomes: `[salt(16) | nonce(12) | ciphertext]` (same as seed encryption in `keypunkd/src/key.rs:39-58`)
- This makes the DB password fully user-derived with no hardcoded values
- Update `encrypt_db()` and `decrypt_db()` to match the new format

### 5c. Paypunkd unlock handler (`paypunkd/src/paypunkd.rs`)

Replace the stub from Step 04 with full implementation:
1. Decrypt `encrypted_db_password` using paypunkd's own keypair + `ephemeral_public_key`
2. Call `db.unlock(decrypted_password)`
3. Check if accounts table has entries via `list_accounts()`
4. If accounts are empty, call `bulk_derive_accounts()`:
   - Forward `encrypted_keypunkd_password` + `keypunkd_client_pk` to keypunkd via `BulkExportViewingKeys` with `count=30` for all registered protocols
   - Save returned viewing keys as Account entries in DB
5. Return `UnlockSuccess { accounts_count }`

### 5d. Paypunkd bulk_derive_accounts handler

Replace the stub with full implementation:
1. Call `keypunk_service.bulk_export_viewing_keys(encrypted_password, client_pk, protocols, 0, count)`
2. For each returned `(protocol, account_index, viewing_key)`:
   - Generate random hex ID (same pattern as `usecases.rs:161-166`)
   - Derive the BIP44 coin type from protocol (e.g., Zcash=133, Ethereum=60)
   - Create `Account { protocol, derivation_path: format!("m/44'/{coin_type}'/{account_index}'"), name: format!("{protocol:?} Account {account_index}"), viewing_key, created_at: now }`
   - Save to DB via `repo.save()`
3. Return all created accounts

### 5e. Paypunkd main.rs

- Remove `config.db_password()` reference (no longer exists)
- Paypunkd starts without opening the DB (or opens in locked state via `Database::open(data_dir)`)
- Store paypunkd's `Keypair` (secret key) on the `Paypunkd` actor for decryption
- Pass `keystore` to `Paypunkd::new()`

### 5f. Update integration test builder

- `TestBuilder::build()` in `tests/tests/integration_test.rs:117` should call `Database::open()` without password, then immediately `unlock()` with the test password to maintain existing test behavior.
- `Paypunkd::new()` now requires `keystore` parameter — pass the keypair

## Cross-cutting concerns

- `Database` struct gains `encrypted_bytes: Option<Vec<u8>>` for locked state
- `Database::open()` must still create `data_dir` if it doesn't exist
- Old encrypted DB files with the hardcoded salt format won't be readable — this is acceptable for pre-alpha
- `Paypunkd::new()` signature changes — update `paypunkd/src/main.rs:75` and `tests/tests/integration_test.rs:118`
- The `respond()` helper in `paypunkd.rs:33-46` can be used for unlock/bulk-derive responses

## Verification

```bash
cargo check
cargo test
# Specifically test DB encryption round-trip:
cargo test -p paypunkd -- --nocapture
```

## Acceptance Criteria

- [ ] `cargo check` succeeds
- [ ] `cargo test` passes (integration tests may need updating for new unlock flow)
- [ ] `Database::open()` with no password succeeds in locked state
- [ ] `Database::unlock("correct-password")` succeeds
- [ ] `Database::unlock("wrong-password")` fails with decryption error
- [ ] `Database::wallet_exists()` returns true when DB file exists
- [ ] No hardcoded `"paypunk-default-password"` string exists anywhere in the codebase
- [ ] Encrypted DB blob uses random salt (not hardcoded)
- [ ] Code is committed with message: "feat: implement DB locked/unlock, remove hardcoded password"
