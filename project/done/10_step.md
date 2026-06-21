# Step 10: Update AssetsScreen — real ETH balance, no fake data

## Context

The AssetsScreen shows the selected account's tokens with balances. For now, only ETH is shown with a real balance fetched from the backend. No hardcoded price data.

## Changes

### `tui/src/screens/assets.rs`

**Constructor:**
```rust
pub fn new(account: AccountInfo) -> Self {
    Self {
        account_id: account.account_id,
        account_name: account.name,
        chain_id: account.chain_id,
        // ...
    }
}
```

**`init()`:**
- Fetch assets via `api.get_assets(&self.account_id)`
- Build list from response

**`render()`:**
- Header: account name + chain
- Send/Receive buttons (as before)
- Body: Single-row table with ETH
  - Columns: Asset | Balance | (Actions via buttons)
  - No price/change columns — show `—` for unavailable data
- Footer: `↑↓ Navigate | ←/→ Buttons | Enter Select | Esc Back`

**Remove:** Hardcoded multi-token table with fake prices.

## Acceptance Criteria

- [ ] AssetsScreen shows account name in header
- [ ] ETH balance is fetched from real backend (or mock)
- [ ] No hardcoded price data
- [ ] Send/Receive buttons navigate to correct screens with account info
- [ ] `cargo build` succeeds
