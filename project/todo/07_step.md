# Step 07: Password-free account creation via bulk derivation

**Prerequisites**: Step 04 (BulkExportViewingKeys message), Step 05 (DB unlock + bulk derive on unlock), Step 06 (API unlock function)

## Goal

Make `create_account` work without requiring a password for pre-derived accounts (indices 0-29). On unlock, viewing keys for accounts 0-29 for all protocols are bulk-derived and stored. Subsequent `create_account` calls for these indices are a simple DB insert. For indices beyond 29, return a clear error asking the user to unlock with a higher count.

## Key files

- `paypunkd/src/messages.rs:40-49` — `CreateAccount` message (remove password fields)
- `paypunkd/src/paypunkd.rs:194-220` — `create_account` handler
- `paypunkd/src/usecases.rs:141-183` — `create_account()` usecase
- `api/src/functions.rs:157-181` — API `create_account()` function
- `api/src/client.rs` — `Client::create_account()` method
- `tests/tests/integration_test.rs:381-462` — `test_create_account`, `test_list_accounts`, `test_get_account_by_id`
- `tui/src/api/real.rs:58-64` — `submit_setup_create()`

## Tasks

### 7a. Update paypunkd unlock handler

In the `unlock()` handler (from Step 05), after decrypting the DB:
- Check if `accounts` table is empty via `list_accounts()`
- If empty, bulk-derive 30 accounts per protocol using keypunkd's `BulkExportViewingKeys`
- Save all accounts to DB
- Return the count as `UnlockSuccess { accounts_count }`

### 7b. Modify `CreateAccount` message (`paypunkd/src/messages.rs`)

Remove `encrypted_password` and `client_public_key` fields from `CreateAccount`:
```rust
CreateAccount {
    protocol: ProtocolId,
    derivation_path: String,
    account_index: u32,
    name: String,
}
```

### 7c. Update `create_account` usecase (`paypunkd/src/usecases.rs`)

`create_account()` no longer calls keypunkd for viewing key export. Instead:
1. Check if an account with the given protocol + derivation_path already exists in DB (use `repo.find_all()` and filter)
2. If yes, return error "account already exists"
3. If no, check if there's a pre-derived viewing key available:
   - For `account_index <= 29`, the viewing key should already be stored from bulk derivation — look it up by protocol + account_index
   - For `account_index > 29`, return error "account index 30 is beyond pre-derived range (0-29). Re-unlock with a higher count to access this account."
4. Generate random hex ID (same pattern as `usecases.rs:161-166`)
5. Create and save Account

### 7d. Update API `create_account` function (`api/src/functions.rs`)

Remove password encryption from `create_account()`:
```rust
pub async fn create_account(
    service: &PaypunkdService,
    protocol: ProtocolId,
    derivation_path: String,
    account_index: u32,
    name: String,
) -> Result<Account, String>
```

### 7e. Update API client (`api/src/client.rs`)

Update `create_account()` signature to not require password:
```rust
pub async fn create_account(
    &self,
    protocol: ProtocolId,
    derivation_path: String,
    account_index: u32,
    name: String,
) -> Result<Account, String>
```

### 7f. Update integration tests

Update `test_create_account` (`tests/tests/integration_test.rs:381`) and `test_list_accounts` (line 406):
- Call `client.generate_seed(password)` first
- Call `client.unlock(password)` — this bulk-derives 30 accounts per protocol
- Then call `client.create_account(...)` **without** password
- Verify accounts are created correctly
- Add a test that `create_account` for index > 29 returns error

### 7g. Update TUI real API (`tui/src/api/real.rs`)

Update `submit_setup_create()` to:
1. Call `client.generate_seed(password)` (existing)
2. Call `client.unlock(password)` (new — this bulk-derives accounts and unlocks DB)
3. Return success

## Cross-cutting concerns

- `PaypunkdRequest::CreateAccount` changes shape — update match arm in `paypunkd/src/paypunkd.rs:286-303`
- `PaypunkService::create_account()` in `paypunkd/src/services.rs:152-176` — remove password params
- `api/src/functions.rs:create_account()` callers in `cli/src/main.rs` and `tui/src/api/real.rs` — update calls
- The pre-derived accounts are stored with deterministic names like "Zcash Account 0", "Zcash Account 1", etc.
- `create_account` without password now uses the pre-derived viewing key from the DB — need a way to find it. Either:
  - Option A: Query accounts by protocol + derivation_path prefix
  - Option B: Store a separate `pre_derived_keys` table
  - Option A is simpler — just check if any account with matching protocol + path prefix exists

## Verification

```bash
cargo check
cargo test
# Specifically run the integration tests:
cargo test -p tests -- --nocnocapture
```

## Acceptance Criteria

- [ ] `cargo check` succeeds
- [ ] `cargo test` passes
- [ ] `create_account` for indices 0-29 succeeds without a password
- [ ] `create_account` for index > 29 returns a clear error message
- [ ] Duplicate account creation returns error
- [ ] After `generate_seed` + `unlock`, accounts 0-29 for all protocols exist in DB
- [ ] Integration tests pass with the new flow
- [ ] Code is committed with message: "feat: password-free account creation via bulk derivation on unlock"
