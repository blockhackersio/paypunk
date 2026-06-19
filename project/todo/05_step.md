# Step 05: Implement database locked/unlock and remove hardcoded password

## Goal

Implement the core unlock flow: paypunkd starts with the DB in locked state, accepts an `Unlock` message with an encrypted password, decrypts the DB, and becomes operational. Remove the hardcoded `"paypunk-default-password"` entirely. On unlock, if no accounts exist, bulk-derive viewing keys from keypunkd.

## Tasks

### 5a. Database changes (`paypunkd/src/database/db.rs`)

- `Database::open(data_dir)` — no password parameter. If encrypted DB exists, open in **locked** state (store the encrypted bytes in memory, don't decrypt yet). If not, create a placeholder.
- Add `Database::unlock(password: &str) -> Result<()>`:
  - Decrypts the stored encrypted bytes with the password
  - Writes plaintext to temp file
  - Opens SQLite connection
  - Runs migrations
- Add `Database::wallet_exists() -> bool` — checks if encrypted DB file exists
- Add `Database::is_locked() -> bool` — returns whether DB is still locked
- Update `Database::close()` — only re-encrypt if DB was unlocked

### 5b. Encryption changes (`paypunkd/src/database/encryption.rs`)

- Remove the hardcoded `DB_SALT` constant
- `derive_db_key()` should use a random salt (stored alongside the ciphertext, like keypunkd's `key.rs` does)
- The encrypted blob format becomes: `[salt(16) | nonce(12) | ciphertext]` (same as seed encryption)
- This makes the DB password fully user-derived with no hardcoded values

### 5c. Paypunkd unlock handler (`paypunkd/src/paypunkd.rs`)

Implement `unlock()`:
1. Decrypt `encrypted_db_password` using paypunkd's own keypair + `ephemeral_public_key`
2. Call `db.unlock(decrypted_password)`
3. Check if accounts table has entries via `list_accounts()`
4. If accounts are empty, call `bulk_derive_accounts()`:
   - Forward `encrypted_keypunkd_password` + `keypunkd_client_pk` to keypunkd via `BulkExportViewingKeys` with `count=30` for all registered protocols
   - Save returned viewing keys as Account entries in DB
5. Return `UnlockSuccess { accounts_count }`

### 5d. Paypunkd bulk_derive_accounts handler

Implement `bulk_derive_accounts()`:
1. Call `keypunk_service.bulk_export_viewing_keys(encrypted_password, client_pk, protocols, 0, count)`
2. For each returned `(protocol, account_index, viewing_key)`:
   - Generate random hex ID
   - Create `Account { protocol, derivation_path: "m/44'/<coin_type>'/<account_index>'", name: format!("{protocol:?} Account {account_index}"), viewing_key, ... }`
   - Save to DB
3. Return all created accounts

### 5e. Paypunkd main.rs

- Remove `config.db_password()` reference
- Paypunkd starts without opening the DB (or opens in locked state)
- Store paypunkd's `Keypair` (secret key) on the `Paypunkd` actor for decryption

### 5f. Update integration test builder

- `TestBuilder::build()` in `tests/tests/integration_test.rs` should call `Database::open()` without password, then immediately `unlock()` with the test password to maintain existing test behavior.

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
