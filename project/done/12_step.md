# Step 12: Update ReceiveScreen — real address from account

## Context

The ReceiveScreen needs to display the real address from the selected account, not a hardcoded stub.

## Changes

### `tui/src/screens/receive.rs`

**Constructor:** Takes `AccountInfo` instead of `chain_id`:
```rust
pub fn new(account: AccountInfo) -> Self {
    Self {
        account_id: account.account_id.clone(),
        chain_id: account.chain_id.clone(),
        // ...
    }
}
```

**Remove chain selector** — only showing one account's address.

**`init()`:** Call `api.receive_state(&self.account_id)`.

**`render()`:** 
- Show address, format, QR payload from `ReceiveData`
- No chain switching arrows

**Footer:** `c Copy Address | Esc Back`

## Acceptance Criteria

- [ ] ReceiveScreen shows real address from selected account
- [ ] No chain selector
- [ ] Copy to clipboard works
- [ ] `cargo build` succeeds
