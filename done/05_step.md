# Step 5: Make Screen trait async + update all screens

## Description

Convert the `Screen` trait to use async methods. Update all screen implementations and the `App` struct to work with async `handle_input` and `handle_paste`.

## Files to modify

- `tui/src/screens/mod.rs` — Add `#[async_trait]`, make `handle_input` and `handle_paste` async
- `tui/src/screens/setup.rs` — Update method signatures
- `tui/src/screens/home.rs` — Update method signatures
- `tui/src/screens/wallets.rs` — Update method signatures
- `tui/src/screens/assets.rs` — Update method signatures
- `tui/src/screens/send.rs` — Update method signatures
- `tui/src/screens/receive.rs` — Update method signatures
- `tui/src/screens/lock.rs` — Update method signatures
- `tui/src/screens/settings.rs` — Update method signatures
- `tui/src/screens/help.rs` — Update method signatures
- `tui/src/screens/component_demo.rs` — Update method signatures
- `tui/src/app.rs` — Update `handle_input` and `handle_paste` to be async, update `process_nav` to be async

## Acceptance Criteria

- [ ] `cargo build` succeeds with no warnings
- [ ] `cargo test` passes all tests
- [ ] `Screen::handle_input` is `async fn`
- [ ] `Screen::handle_paste` is `async fn`
- [ ] `App::handle_input` and `App::handle_paste` are `async fn`

## Detailed Steps

1. Open `tui/src/screens/mod.rs`. Add `use async_trait::async_trait;`. Add `#[async_trait]` above `pub trait Screen`. Make `handle_input` and `handle_paste` async:
   ```rust
   #[async_trait]
   pub trait Screen {
       fn name(&self) -> &str;
       fn init(&mut self, _api: &dyn WalletApi) {}
       fn on_reactivate(&mut self, _api: &mut dyn WalletApi) {}
       fn render(&mut self, frame: &mut Frame, api: &dyn WalletApi);
       async fn handle_input(&mut self, key: crossterm::event::KeyEvent, api: &mut dyn WalletApi) -> Nav;
       async fn handle_paste(&mut self, _text: &str, _api: &mut dyn WalletApi) -> Nav { Nav::None }
   }
   ```

2. Open `tui/src/app.rs`. Add `use async_trait::async_trait;`. Update `handle_input` and `handle_paste` to be `async fn`. Update `process_nav` to be `async fn` (since it calls `screen.init()` which is sync, this is fine):
   ```rust
   pub async fn handle_input(&mut self, key: crossterm::event::KeyEvent) -> io::Result<()> {
       let api: &mut dyn WalletApi = &mut *self.api;
       let nav = if let Some(screen) = self.screen_stack.last_mut() {
           screen.handle_input(key, api).await
       } else {
           Nav::None
       };
       self.process_nav().await;
       Ok(())
   }

   pub async fn handle_paste(&mut self, text: &str) {
       let api: &mut dyn WalletApi = &mut *self.api;
       let nav = if let Some(screen) = self.screen_stack.last_mut() {
           screen.handle_paste(text, api).await
       } else {
           Nav::None
       };
       self.process_nav().await;
   }

   async fn process_nav(&mut self, nav: Nav) {
       // ... same body as before
   }
   ```

3. For each screen file (setup, home, wallets, assets, send, receive, lock, settings, help, component_demo), add `use async_trait::async_trait;` at the top, and add `async` before `fn handle_input` and `fn handle_paste`.

4. Run `cargo build` and fix any compilation errors.

5. Run `cargo test` and verify all tests pass.

## Completion

When this step is complete — **STOP. DO NOT START THE NEXT STEP.**

```bash
git add -A && git commit -m "step 5: make Screen trait async + update all screens"

mv todo/05_step.md done/

cat >> todo/goal.md << 'EOF'

## Step 5 — Done

Made `Screen` trait async with `#[async_trait]`. Updated all 10 screen implementations and `App` struct for async handle_input/handle_paste.
EOF
```

## ⛔ STOP HERE. WAIT FOR INSTRUCTION. DO NOT READ OR EXECUTE STEP 6.
