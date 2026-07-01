# Step 3: Wire lock screen authentication

## Issue
**#2** — `submit_lock()` in `tui/src/api/real.rs:523` always returns `Ok(())`. No actual authentication check. Any password unlocks.
**#10** — `get_lock()` in `tui/src/api/real.rs:514` always returns hardcoded `LockData { password_set: true, failed_attempts: 0 }`.

## What to do

1. **Add IPC messages** in `paypunkd/src/messages.rs`:
   - `GetLockState` — returns whether password is set and failed attempt count
   - `VerifyPassword { encrypted_password: Vec<u8>, ephemeral_public_key: [u8; 32] }` — verifies password against keypunkd (re-uses the existing unlock path's password verification)

2. **Implement handlers** in `paypunkd/src/paypunkd.rs`:
   - `get_lock_state()` — query keypunkd for whether seed is encrypted (password is set), return state
   - `verify_password()` — forward to keypunkd for password verification (use existing `has_seed` + attempt a lightweight decrypt verification)

3. **Add IPC message in keypunkd** (`keypunkd/src/messages.rs`):
   - `VerifyPassword { encrypted_password: Vec<u8>, client_public_key: [u8; 32] }` — decrypts the password, attempts to decrypt `seed.enc`, returns success/failure
   - Response: `PasswordVerified` or `Error`

4. **Wire RealWalletApi** (`tui/src/api/real.rs`):
   - `get_lock()` — call IPC `GetLockState` instead of returning hardcoded data
   - `submit_lock()` — call IPC `VerifyPassword` instead of always returning `Ok(())`
   - Track `failed_attempts` on the paypunkd side (in-memory is fine for now)

5. **Add the IPC methods to `PaypunkService`** (`paypunkd/src/services.rs`).

6. **Add the IPC methods to the api `Client`** (`api/src/client.rs` and `api/src/functions.rs`).

## Verification
- `cargo build` succeeds
- `cargo test` passes
- `submit_lock()` with wrong password returns an error
- `submit_lock()` with correct password returns `Ok(())`
- `get_lock()` returns `password_set: true` when seed exists, `false` otherwise
