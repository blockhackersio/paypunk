# Step 07: Password-free account creation via bulk derivation

## Goal

Make `create_account` work without requiring a password for pre-derived accounts (indices 0-29). On unlock, viewing keys for accounts 0-29 for all protocols are bulk-derived and stored. Subsequent `create_account` calls for these indices are a simple DB insert. For indices beyond 29, return a clear error asking the user to unlock with a higher count.

## Tasks

### 7a. Update paypunkd unlock handler

In the `unlock()` handler (from Step 05), after decrypting the DB:
- Check if `accounts` table is empty
- If empty, bulk-derive 30 accounts per protocol using keypunkd's `BulkExportViewingKeys`
- Save all accounts to DB
- Return the count

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
1. Check if an account with the given protocol + derivation_path already exists in DB
2. If yes, return error "account already exists"
3. If no, check if there's a pre-derived viewing key available:
   - For account_index <= 29, the viewing key should already be stored from bulk derivation
   - For account_index > 29, return error "account index beyond pre-derived range, please unlock with a higher count"
4. Generate random hex ID
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

Update `create_account()` signature to not require password.

### 7f. Update integration tests

Update `test_create_account` and `test_list_accounts` in `tests/tests/integration_test.rs`:
- Generate seed + unlock first (which bulk-derives accounts)
- Then call `create_account` without password
- Verify accounts are created correctly

### 7g. Update TUI real API (`tui/src/api/real.rs`)

Update `submit_setup_create()` to:
1. Call `client.generate_seed(password)` (existing)
2. Call `client.unlock(password)` (new — this bulk-derives accounts)
3. Return success

## Acceptance Criteria

- [ ] `cargo check` succeeds
- [ ] `cargo test` passes
- [ ] `create_account` for indices 0-29 succeeds without a password
- [ ] `create_account` for index > 29 returns a clear error message
- [ ] Duplicate account creation returns error
- [ ] After `generate_seed` + `unlock`, accounts 0-29 for all protocols exist in DB
- [ ] Integration tests pass with the new flow
- [ ] Code is committed with message: "feat: password-free account creation via bulk derivation on unlock"
