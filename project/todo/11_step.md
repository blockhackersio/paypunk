# Step 11: Update SendScreen — password on Review, simplified flow

## Context

The SendScreen flow is simplified to: Form → Review (with password) → Sending → Confirmed. The separate ConfirmSend step is removed. Fee data and nonce appear only on the Review step (after `submit_intent` builds the transaction). The password is entered on the Review step.

## Changes

### `tui/src/screens/send.rs`

**Remove `SendStep::ConfirmSend` variant** — only Form, Review, Sending, Confirm remain.

**Constructor:** Takes `AccountInfo` instead of just `chain_id`:
```rust
pub fn new(account: AccountInfo) -> Self {
    Self {
        account_id: account.account_id.clone(),
        chain_id: account.chain_id.clone(),
        // ...
    }
}
```

**Add password field:**
```rust
password_field: TextField,  // added to struct
```

**Form step:**
- Show to address field, amount field
- Show balance from `SendData.spendable_balance` (formatted as ETH)
- No fee data, no nonce, no fee tier selector
- Enter → calls `submit_send_review()`, transitions to Review

**Review step (replaces old Review + ConfirmSend):**
- Show all details: from, to, amount, fee (formatted as ETH), nonce, total
- Show password field below the details
- Enter → reads password, calls `submit_send_confirm()`, transitions to Sending
- Esc → back to Form

**Sending step:** Unchanged.

**Confirmed step:** Unchanged.

**Remove:**
- `fee_tiers` SelectList
- `confirm_choice` SelectList
- `focus` tracking for fee tier
- Default address/amount values
- Hardcoded `"face-id-assertion-token"`

**Update `handle_input()`:**
- Form: Tab/Down cycles through to_field, amount_field
- Review: password_field is focused; Enter submits; Esc goes back
- No ConfirmSend handling

**Update `render_form()`:**
- Show balance line
- No fee tier, no nonce display

**Update `render_review()`:**
- Show all details including nonce
- Render password field
- No "Press ENTER to confirm" — show "Enter password and press ENTER to send"

## Acceptance Criteria

- [ ] Form step shows to/amount fields and balance only
- [ ] Review step shows all details (to, amount, fee, nonce, total) + password field
- [ ] No separate ConfirmSend step
- [ ] Password is sent with `submit_send_confirm`
- [ ] No hardcoded addresses, amounts, or biometric tokens
- [ ] `cargo build` succeeds
