# Step 13: Update GreetingScreen and SetupScreen navigation

## Context

After wallet unlock or creation, the TUI should navigate to the redesigned HomeScreen (not WalletsScreen). This step updates the navigation targets.

## Changes

### `tui/src/screens/greeting.rs`

**Line 90:** Change:
```rust
Ok(_data) => return Nav::Replace(Box::new(WalletsScreen::new())),
```
To:
```rust
Ok(_data) => return Nav::Replace(Box::new(HomeScreen::new())),
```

### `tui/src/screens/setup.rs`

**Line 254** (create wallet success): Change:
```rust
return Nav::Replace(Box::new(WalletsScreen::new()));
```
To:
```rust
return Nav::Replace(Box::new(HomeScreen::new()));
```

**Line 299** (import wallet success): Change:
```rust
return Nav::Replace(Box::new(WalletsScreen::new()));
```
To:
```rust
return Nav::Replace(Box::new(HomeScreen::new()));
```

## Acceptance Criteria

- [ ] After unlocking wallet, user lands on HomeScreen (accounts list)
- [ ] After creating new wallet, user lands on HomeScreen
- [ ] After importing wallet, user lands on HomeScreen
- [ ] `cargo build` succeeds
