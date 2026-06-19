# Step 06: Add API functions and TUI GreetingScreen

**Prerequisites**: Step 04 (IPC messages exist), Step 05 (DB unlock implemented)

## Goal

Add the public API functions for unlock and wallet-existence check. Add the GreetingScreen to the TUI with conditional startup: if wallet exists, show greeting (password entry); if not, show setup wizard.

## Key files

- `api/src/functions.rs:1-196` — all API functions
- `api/src/client.rs:1-50` — `Client` struct with methods
- `tui/src/api/mod.rs:8-39` — `WalletApi` trait
- `tui/src/api/types.rs:1-295` — TUI data types
- `tui/src/api/real.rs:1-299` — `RealWalletApi` impl
- `tui/src/api/mock.rs` — `MockWalletApi` impl
- `tui/src/lib.rs:24-104` — `run_tui()` startup logic
- `tui/src/screens/setup.rs:1-538` — existing SetupScreen (reference for new screen)

## Tasks

### 6a. API functions (`api/src/functions.rs`)

Add:
```rust
pub async fn check_wallet_exists(service: &PaypunkService) -> Result<bool, String>
pub async fn unlock(
    service: &PaypunkService,
    password: Zeroizing<String>,
) -> Result<u32, String>
```
- `check_wallet_exists`: sends `HasSeed` to paypunkd, which proxies to keypunkd
- `unlock`:
  1. Creates ephemeral keypair
  2. Fetches keypunkd's public key from paypunkd (existing `get_keypunk_encryption_key`)
  3. Fetches paypunkd's public encryption key (new `get_paypunkd_encryption_key`)
  4. Encrypts password to paypunkd's key (for DB unlock)
  5. Encrypts password to keypunkd's key (for bulk derivation)
  6. Sends `Unlock` to paypunkd with both encrypted payloads
  7. Returns accounts count from `UnlockSuccess`

### 6b. API client (`api/src/client.rs`)

Add methods:
- `check_wallet_exists() -> Result<bool, String>`
- `unlock(password: Zeroizing<String>) -> Result<u32, String>`
- `get_paypunkd_encryption_key() -> Result<[u8; 32], String>` (wraps service method)

### 6c. TUI API trait (`tui/src/api/mod.rs`)

Add to `WalletApi`:
```rust
async fn check_wallet_exists(&self) -> bool;
async fn unlock(&self, password: String) -> Result<UnlockData, ApiError>;
```

### 6d. TUI API types (`tui/src/api/types.rs`)

Add:
```rust
pub struct UnlockData {
    pub accounts_count: u32,
}
```

### 6e. TUI real API (`tui/src/api/real.rs`)

Implement `check_wallet_exists()` and `unlock()` using `paypunk_api::Client`:
- `check_wallet_exists`: calls `self.client.check_wallet_exists().await.unwrap_or(false)`
- `unlock`: calls `self.client.unlock(Zeroizing::new(password)).await`, maps result

### 6f. TUI mock API (`tui/src/api/mock.rs`)

Implement mock versions:
- `check_wallet_exists()` — return `false` (always show setup in mock mode)
- `unlock()` — return `Ok(UnlockData { accounts_count: 2 })` (simulate 2 pre-derived accounts)

### 6g. New GreetingScreen (`tui/src/screens/greeting.rs`)

Create a new screen (model after `tui/src/screens/lock.rs` for reference):
- Title: "PayPunk Wallet"
- Subtitle: "Enter your password to unlock"
- Single password field (masked input, use `TextField` with `password_mode: true`)
- Submit on Enter → calls `api.unlock(password)`
- On success → navigates to `WalletsScreen` (or `HomeScreen`)
- On error → shows error message
- Footer with help: "Enter to unlock, Ctrl+C to quit"
- Register in `tui/src/screens/mod.rs`

### 6h. TUI startup (`tui/src/lib.rs`)

Update `run_tui()`:
1. Call `api.check_wallet_exists()` before pushing any screen
2. If `true` → push `GreetingScreen`
3. If `false` → push `SetupScreen` (existing behavior)
4. If API call fails → fall back to `SetupScreen` (fresh start assumption)

## Cross-cutting concerns

- `PaypunkService` needs the `get_paypunkd_encryption_key()` method — add in `paypunkd/src/services.rs` (stub already from Step 04)
- `Client` in `api/src/client.rs` needs a `get_paypunkd_encryption_key` method that calls the service
- The TUI `WalletApi` trait is `#[async_trait(?Send)]` — new methods must match
- GreetingScreen needs to be added to `tui/src/screens/mod.rs` exports
- After unlock, the user should land on `WalletsScreen` (which lists accounts)

## Verification

```bash
cargo check
cargo test
# Manual: run with mock API to test greeting screen
# Manual: run with real API + daemons to test unlock flow
```

## Acceptance Criteria

- [ ] `cargo check` succeeds
- [ ] `cargo test` passes
- [ ] `api::Client::unlock()` sends encrypted password to both paypunkd and keypunkd
- [ ] `api::Client::check_wallet_exists()` returns true when seed.enc exists on keypunkd
- [ ] TUI shows GreetingScreen when wallet exists
- [ ] TUI shows SetupScreen when wallet does not exist
- [ ] GreetingScreen password entry navigates to wallets on success
- [ ] GreetingScreen shows error message on wrong password
- [ ] Mock API compiles and works for development
- [ ] Code is committed with message: "feat: add API unlock functions and TUI GreetingScreen"
