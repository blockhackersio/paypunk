# Step 06: Add API functions and TUI GreetingScreen

## Goal

Add the public API functions for unlock and wallet-existence check. Add the GreetingScreen to the TUI with conditional startup: if wallet exists, show greeting (password entry); if not, show setup wizard.

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
  2. Fetches keypunkd's public key from paypunkd
  3. Fetches paypunkd's public encryption key (new message)
  4. Encrypts password to paypunkd's key (for DB unlock)
  5. Encrypts password to keypunkd's key (for bulk derivation)
  6. Sends `Unlock` to paypunkd with both encrypted payloads
  7. Returns accounts count

### 6b. API client (`api/src/client.rs`)

Add methods:
- `check_wallet_exists() -> Result<bool, String>`
- `unlock(password: Zeroizing<String>) -> Result<u32, String>`

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

Implement `check_wallet_exists()` and `unlock()` using `paypunk_api::Client`.

### 6f. TUI mock API (`tui/src/api/mock.rs`)

Implement mock versions (return hardcoded values for development).

### 6g. New GreetingScreen (`tui/src/screens/greeting.rs`)

A new screen with:
- Title: "PayPunk Wallet"
- Subtitle: "Enter your password to unlock"
- Single password field (masked input)
- Submit button / Enter key → calls `api.unlock(password)`
- On success → navigates to `WalletsScreen` (or `HomeScreen`)
- On error → shows error message
- Footer with help: "Enter to unlock, Ctrl+C to quit"

### 6h. TUI startup (`tui/src/lib.rs`)

Update `run_tui()`:
1. Call `api.check_wallet_exists()`
2. If `true` → push `GreetingScreen`
3. If `false` → push `SetupScreen` (existing behavior)
4. If API call fails → fall back to `SetupScreen` (fresh start assumption)

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
