# Step 10: TUI Screen Modifications

## Goal
Modify existing TUI screens: add History button and sync to Assets, add memo field to Send, add birthday height to Setup.

## Changes

### 1. `tui/src/screens/assets.rs`

**Add History button**: Add a third button alongside Send/Receive.

Update the button section:
```rust
use crate::screens::history::HistoryScreen;

// In the Buttons focus handling:
enum AssetsFocus {
    Buttons(usize),
    Table,
}

// In init(), store the protocol string for sync:
struct AssetsScreen {
    account: AccountInfo,
    data: Option<AssetsData>,
    list: List<AssetAction>,
    focus: AssetsFocus,
    protocol: String, // "Zcash" or "Ethereum"
    sync_status: SyncStatus,
}
```

**In `render()`**, add History button:
```rust
let mut hist_btn = Button::new(" \u{2191} History ").size(ButtonSize::Sm);
hist_btn.set_focused(on_buttons && matches!(self.focus, AssetsFocus::Buttons(2)));

// Add to btn_bar:
.child_with(Constraint::Length(12), hist_btn)
```

Update button count references: Left/Right cycles through 3 buttons (0, 1, 2).

**In `handle_input()`**, update Enter handler:
```rust
KeyCode::Enter => {
    return match *sel {
        0 => Nav::Push(Box::new(SendScreen::new(self.account.clone()))),
        1 => Nav::Push(Box::new(ReceiveScreen::new(self.account.clone()))),
        2 => Nav::Push(Box::new(HistoryScreen::new(
            self.account.account_id.clone(),
            self.account.name.clone(),
        ))),
        _ => Nav::None,
    };
}
```

**Add sync status display** below account info:
```rust
// After the subtitle, show sync status
if self.sync_status.is_syncing {
    let sync_line = Paragraph::new(
        Line::from(vec![theme.warning(format!(
            " Syncing: {} / {} blocks ",
            self.sync_status.current_height,
            self.sync_status.target_height,
        ))]),
    ).style(Style::new().bg(ui::BG));
    frame.render_widget(sync_line, /* area below subtitle */);
}
```

**Wire `r` key** to trigger sync:
```rust
KeyCode::Char('r') => {
    // Trigger sync for this protocol
    let protocol = if self.account.chain_id.contains("eip155") {
        "Ethereum"
    } else {
        "Zcash"
    };
    let _ = api.sync(protocol).await;
    // Don't navigate — stay on assets screen
}
```

**Poll sync status** in `tick()`:
```rust
async fn tick(&mut self, api: &mut dyn WalletApi) {
    let protocol = if self.account.chain_id.contains("eip155") {
        "Ethereum"
    } else {
        "Zcash"
    };
    self.sync_status = api.get_sync_status(protocol).await;
}
```

Update footer help text to include `r` for refresh and `h` for history:
```rust
let footer_text = theme.help_line([
    ("\u{2191}\u{2193}", "Navigate"),
    ("\u{2190}/\u{2192}", "Buttons"),
    ("Enter", "Select action"),
    ("r", "Refresh/Sync"),
    ("Esc", "Back to wallets"),
    ("?", "Help"),
]);
```

Add `use crate::api::types::SyncStatus;` to imports.

### 2. `tui/src/screens/send.rs`

**Add memo field** (shown only for Zcash):

Add field to `SendScreen`:
```rust
memo_field: TextField,
```

Initialize in constructor:
```rust
memo_field: TextField::new(TextFieldConfig {
    label: "Memo (optional)".into(),
    placeholder: "Enter memo (Zcash only)...".into(),
    password_mode: false,
    initial_value: String::new(),
    feedback: None,
}),
```

Update `max_focus` to include memo when Zcash:
```rust
// In handle_input, Form step:
let is_zcash = self.chain_id.contains("bip122") || !self.chain_id.contains("eip155");
let max_focus = if is_zcash { 2 } else { 1 }; // 0=to, 1=amount, 2=memo
```

**Render memo field** in `render_form()`:
```rust
if is_zcash {
    self.memo_field.set_focused(self.focus == 2);
    self.memo_field.render(
        frame,
        inner.inner(Margin { vertical: 9, horizontal: 2 }),
    );
}
```

**Pass memo** in `submit_send_review()` call. In `handle_input` Enter handler for Form step:
```rust
let memo = if is_zcash {
    Some(self.memo_field.value().to_string())
} else {
    None
};
```

Update the `SendReviewInput` to include memo. Add a `memo: Option<String>` field to `SendReviewInput` in `tui/src/api/types.rs`.

**ZEC amount validation**: In `render_form()`, show ZEC-specific help text:
```rust
let symbol = if data.chain_id.contains("eip155") {
    "ETH"
} else {
    "ZEC"
};
// Show decimal precision hint for Zcash
if !is_zcash {
    // existing balance display
} else {
    // balance display with ZEC-specific formatting
    let bal_line = Line::from(vec![
        theme.muted("Balance: "),
        theme.span(format!("{} {}", bal_str, symbol)),
        theme.muted(" (max 8 decimals)"),
    ]);
}
```

### 3. `tui/src/screens/setup.rs`

No changes needed here — the birthday height prompt is for Zcash account creation, which happens in the Home screen's "Add Account" flow, not during initial setup. The `add_account()` method in `RealWalletApi` and `MockWalletApi` needs to be updated to prompt for birthday height.

### 4. `tui/src/api/types.rs`

Add `memo` field to `SendReviewInput`:
```rust
#[derive(Debug, Clone)]
pub struct SendReviewInput {
    pub to_address: String,
    pub amount: String,
    pub token_id: String,
    pub chain_id: String,
    pub account_id: String,
    pub memo: Option<String>,
}
```

### 5. `tui/src/api/real.rs`

Update `submit_send_review()` to pass memo:
```rust
ProtocolId::Zcash => Intent::Zcash(paypunk_types::ZcashIntent::Transfer {
    to: input.to_address.clone(),
    amount: input.amount.clone(),
    from: from_address,
    asset,
    memo: input.memo.clone(),
}),
```

### 6. `tui/src/api/mock.rs`

Update `submit_send_review()` to accept the new `memo` field (ignore it).

## Verification
- `cargo build -p paypunk-tui` succeeds
