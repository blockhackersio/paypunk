# Step 5: Update TUI data types

## Context

The TUI data types need to support account-based operations (sending/receiving from a specific account) and the simplified send flow (password on Review, fee data only on Review).

## Changes

### `tui/src/api/types.rs`

**Update `AccountInfo`:**
```rust
pub struct AccountInfo {
    pub account_id: String,
    pub name: String,
    pub address: String,
    pub chain_id: String,
    pub protocol: String,
}
```

**Update `HomeData`:** Remove balances, keep accounts:
```rust
pub struct HomeData {
    pub accounts: Vec<AccountInfo>,
    pub fiat_currency: String,
}
```

**Update `SendData`:** Add `account_id`, remove fee data types:
```rust
pub struct SendData {
    pub account_id: String,
    pub from_address: String,
    pub spendable_balance: String,
    pub decimals: u8,
    pub chain_id: String,
}
```

**Update `SendReviewData`:** Add `nonce`:
```rust
pub struct SendReviewData {
    pub to_address: String,
    pub amount: String,
    pub fee_estimate: String,
    pub total_amount: String,
    pub chain_id: String,
    pub nonce: u64,
}
```

**Update `SendReviewInput`:** Add `account_id`:
```rust
pub struct SendReviewInput {
    pub to_address: String,
    pub amount: String,
    pub token_id: String,
    pub chain_id: String,
    pub fee_selection: FeeSelection,
    pub account_id: String,
}
```

**Remove unused types:**
- `FeeData` / `FeeDataEth` / `FeeRates` / `UtxoInfo` — fee data is now shown only on Review step
- `PendingTx` — not used in new HomeScreen design

**Keep `AuthConfirmation` as-is** (auth_type will always be "password").

**Update `ReceiveData`:** Add `account_id`:
```rust
pub struct ReceiveData {
    pub address: String,
    pub chain_id: String,
    pub address_format: String,
    pub qr_payload: String,
    pub account_id: String,
}
```

## Acceptance Criteria

- [ ] New types compile
- [ ] Old unused types removed
- [ ] `cargo build` in tui crate succeeds
