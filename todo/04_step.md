# Step 4: Make WalletApi trait async

## Description

Convert the `WalletApi` trait in the TUI from synchronous to async using the `async-trait` proc macro. Update `MockWalletApi` to match. Add `tokio` and `async-trait` dependencies to the TUI crate.

## Files to modify

- `tui/Cargo.toml` — Add `tokio` and `async-trait` dependencies
- `tui/src/api/mod.rs` — Add `#[async_trait]` to `WalletApi` trait, make all methods `async fn`
- `tui/src/api/mock.rs` — Add `#[async_trait]` impl, wrap all methods with `async fn`

## Acceptance Criteria

- [ ] `cargo build` succeeds with no warnings
- [ ] `cargo test` passes all tests
- [ ] `WalletApi` methods are all `async fn`
- [ ] `MockWalletApi` compiles with the new async trait

## Detailed Steps

1. Open `tui/Cargo.toml`. Add to `[dependencies]`:
   ```toml
   async-trait = { workspace = true }
   tokio = { workspace = true, features = ["rt", "macros", "sync"] }
   ```

2. Open `tui/src/api/mod.rs`. Add `use async_trait::async_trait;`. Add `#[async_trait]` above `pub trait WalletApi`. Make every method `async fn`:
   ```rust
   #[async_trait]
   pub trait WalletApi {
       async fn get_setup(&self) -> SetupData;
       async fn submit_setup_create(&self, input: SetupCreateInput) -> Result<(), ApiError>;
       // ... etc for all methods
   }
   ```

3. Open `tui/src/api/mock.rs`. Add `use async_trait::async_trait;`. Add `#[async_trait]` above `impl WalletApi for MockWalletApi`. Add `async` before every `fn`:
   ```rust
   #[async_trait]
   impl WalletApi for MockWalletApi {
       async fn get_setup(&self) -> SetupData { /* existing body */ }
       async fn submit_setup_create(&self, _input: SetupCreateInput) -> Result<(), ApiError> { Ok(()) }
       // ... etc
   }
   ```

4. Run `cargo build` and fix any compilation errors. The compiler will point out any methods that were missed.

5. Run `cargo test` and verify all tests pass.

## Completion

When this step is complete — **STOP. DO NOT START THE NEXT STEP.**

```bash
git add -A && git commit -m "step 4: make WalletApi trait async"

mv todo/04_step.md done/

cat >> todo/goal.md << 'EOF'

## Step 4 — Done

Made `WalletApi` trait async with `#[async_trait]`. Updated `MockWalletApi`. Added `tokio` + `async-trait` deps to TUI crate.
EOF
```

## ⛔ STOP HERE. WAIT FOR INSTRUCTION. DO NOT READ OR EXECUTE STEP 5.
