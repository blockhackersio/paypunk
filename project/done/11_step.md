# Step 11: TUI App Wiring + Global Sync Status Bar

## Goal
Register HistoryScreen navigation, add sync status bar to main render loop, and wire birthday height into account creation.

## Changes

### 1. `tui/src/app.rs`

Add `Nav::History` variant:
```rust
pub enum Nav {
    None,
    Push(Box<dyn Screen>),
    Pop,
    Replace(Box<dyn Screen>),
    Quit,
    History(String), // account_id
}
```

Handle in `process_nav()`:
```rust
Nav::History(account_id) => {
    // Look up account info and push HistoryScreen
    let accounts = self.api.list_accounts().await;
    if let Ok(accs) = accounts {
        if let Some(acc) = accs.iter().find(|a| a.account_id == account_id) {
            let mut screen = Box::new(crate::screens::history::HistoryScreen::new(
                account_id,
                acc.name.clone(),
            ));
            screen.init(&*self.api).await;
            self.push_screen(screen);
        }
    }
}
```

Add `use crate::screens::history::HistoryScreen;` import.

### 2. `tui/src/lib.rs`

Add sync status bar to the main `render()` function. After rendering the current screen, render a 1-line status bar at the bottom showing sync progress:

```rust
fn render(frame: &mut Frame, app: &mut App) {
    let api: &dyn WalletApi = &*app.api;

    let bg_block = Block::new().style(Style::new().bg(ui::BG));
    frame.render_widget(bg_block, frame.area());

    if let Some(screen) = app.screen_stack.last_mut() {
        // Reserve space for sync status bar at the bottom
        let area = frame.area();
        let main_area = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: area.height.saturating_sub(1), // leave room for status bar
        };
        let status_area = Rect {
            x: area.x,
            y: area.y + area.height.saturating_sub(1),
            width: area.width,
            height: 1,
        };

        // Render screen in main area
        // Note: this requires modifying the render calls to pass main_area
        // or the screen's render() needs to accept the area
        screen.render(frame, api);

        // Render sync status bar
        render_sync_status(frame, status_area, api);
    } else {
        let block = Block::new().style(Style::new().bg(ui::BG));
        frame.render_widget(block, frame.area());
        let msg = Paragraph::new(Line::from("No screen loaded").centered())
            .style(Style::new().fg(ui::palette().error));
        frame.render_widget(msg, frame.area());
    }
}

fn render_sync_status(frame: &mut Frame, area: Rect, api: &dyn WalletApi) {
    // Check sync status for Zcash protocol
    let status = api.get_sync_status("Zcash").await;
    // Note: render is synchronous, so we need a different approach
    // Store sync status in App and update it in tick()
}
```

**Better approach**: Store the sync status in the `App` struct and update it in `tick()`:

Add to `App`:
```rust
pub struct App {
    pub screen_stack: Vec<Box<dyn Screen>>,
    pub api: Box<dyn WalletApi>,
    pub should_quit: bool,
    pub sync_status: SyncStatus,
}
```

Initialize in `App::new()`:
```rust
sync_status: SyncStatus::default(),
```

Update in `tick()`:
```rust
pub async fn tick(&mut self) {
    let api: &mut dyn WalletApi = &mut *self.api;
    if let Some(screen) = self.screen_stack.last_mut() {
        screen.tick(api).await;
    }
    // Poll sync status for Zcash
    self.sync_status = api.get_sync_status("Zcash").await;
}
```

In `render()`:
```rust
fn render(frame: &mut Frame, app: &mut App) {
    // ... existing rendering ...

    // Render sync status bar at bottom
    if app.sync_status.is_syncing {
        let status_area = Rect {
            x: frame.area().x,
            y: frame.area().y + frame.area().height.saturating_sub(1),
            width: frame.area().width,
            height: 1,
        };
        let theme = ui::theme();
        let status_text = format!(
            " Syncing Zcash: {} / {} blocks ",
            app.sync_status.current_height,
            app.sync_status.target_height,
        );
        let status_line = Paragraph::new(Line::from(vec![
            theme.warning(&status_text),
        ]));
        let status_block = Block::new().style(Style::new().bg(ui::SURFACE));
        frame.render_widget(status_block, status_area);
        frame.render_widget(status_line, status_area);
    }
}
```

Add `use crate::api::types::SyncStatus;` to imports.

### 3. Birthday Height in Account Creation

The birthday height should be prompted when adding a Zcash account. This happens in the `add_account()` flow.

**In `tui/src/screens/home.rs`**, modify the Add Account flow. When the user presses `a` to add an account, if the next account would be Zcash, show a birthday height field.

Alternatively, add a new method to the `WalletApi` trait:

```rust
async fn add_zcash_account(&self, birthday_height: u64) -> Result<(), ApiError>;
```

In `RealWalletApi`:
```rust
async fn add_zcash_account(&self, birthday_height: u64) -> Result<(), ApiError> {
    let accounts = self.client.list_accounts().await.map_err(ApiError)?;
    let zcash_count = accounts.iter().filter(|a| a.protocol == ProtocolId::Zcash).count();
    let path = self.client.derivation_path(ProtocolId::Zcash, zcash_count as u32);
    let name = format!("Zcash Account {zcash_count}");
    self.client
        .create_account(
            ProtocolId::Zcash,
            path,
            zcash_count as u32,
            name,
            Some(birthday_height),
        )
        .await
        .map_err(ApiError)?;
    Ok(())
}
```

In `MockWalletApi`:
```rust
async fn add_zcash_account(&self, _birthday_height: u64) -> Result<(), ApiError> {
    // Same as existing add_account but for Zcash
    self.add_account().await
}
```

## Verification
- `cargo build -p paypunk-tui` succeeds
