# Step 9: Redesign HomeScreen — accounts list, no balance, Add Account

## Context

The HomeScreen needs to show a list of accounts (name + address) with no balance data. The user can select an account to view its assets, send from it, or receive to it. An "Add Account" button creates the next account index.

## Changes

### `tui/src/screens/home.rs`

**Constructor:** No arguments (fetches accounts from API on init).

**State:**
```rust
pub struct HomeScreen {
    list: List<AccountAction>,  // simple list of account rows
    state: ApiState<HomeData>,
}
```

**`init()`:** Call `api.home_state()` to load accounts.

**`render()`:**
- Header: "PayPunk Wallet" title
- Body: List of accounts. Each row shows:
  - Account name (e.g., "Ethereum Account 0")
  - Address (truncated, e.g., "0x742d...8f44e")
- Footer: `↑↓ Select | Enter Assets | s Send | o Receive | a Add Account | r Refresh | q Quit`

**`handle_input()`:**
- `↑/↓`: Navigate list
- `Enter`: Navigate to `AssetsScreen::new(selected_account_id)`
- `s`: Navigate to `SendScreen::new(selected_account_info)`
- `o`: Navigate to `ReceiveScreen::new(selected_account_info)`
- `a`: Call `api.add_account().await`, then refresh list
- `r`: Call `api.refresh_home().await`, reload
- `q`: Quit

**Remove:** Menu popup, balance rendering, `BalanceItem` usage.

### `tui/src/components/` (if needed)
- Create a simple `AccountItem` component or use existing `LabelItem` for rendering account rows

## Acceptance Criteria

- [ ] HomeScreen shows accounts with name and address (no balances)
- [ ] Selecting an account and pressing Enter navigates to AssetsScreen
- [ ] `s` key navigates to SendScreen with account info
- [ ] `o` key navigates to ReceiveScreen with account info
- [ ] `a` key creates a new account and refreshes the list
- [ ] `cargo build` succeeds
