# Step 8: Update MockWalletApi

## Context

The `MockWalletApi` needs to match the new `WalletApi` trait signatures and provide realistic mock data for TUI development without a running backend.

## Changes

### `tui/src/api/mock.rs`

**Add mock data:**
```rust
struct MockData {
    accounts: Vec<AccountInfo>,
    next_account_index: u32,
}
```

**Update all method signatures to match new trait:**

- `get_home()`: Return mock accounts (Ethereum + Zcash) with names and addresses, no balances
- `list_accounts()`: Return mock `AccountInfo` list
- `add_account()`: Create a new mock account with incremented index
- `get_send(account_id)`: Return mock send data for that account
- `get_receive(account_id)`: Return mock receive data with that account's address
- `get_assets(account_id)`: Return single ETH row with mock balance
- `send_state(account_id)` / `refresh_send(account_id)`: Use account_id for caching
- `receive_state(account_id)` / `refresh_receive(account_id)`: Use account_id for caching

**Remove:** `get_wallets()` method

## Acceptance Criteria

- [ ] All trait methods implemented with new signatures
- [ ] Mock data is realistic (valid Ethereum addresses, reasonable balances)
- [ ] `add_account()` increments account index
- [ ] `cargo build` succeeds
- [ ] TUI runs with mock API without errors
