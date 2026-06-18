# Step 3: Add broadcast to api crate

## Description

Expose `broadcast_transaction` through the public-facing `paypunk-api` crate so that consumers (CLI, TUI, etc.) can broadcast signed transactions via paypunkd.

## Files to modify

- `api/src/functions.rs` — Add `broadcast_transaction` function
- `api/src/client.rs` — Add `broadcast_transaction` method on `Client`

## Acceptance Criteria

- [ ] `cargo build` succeeds with no warnings
- [ ] `cargo test` passes all tests
- [ ] `Client::broadcast_transaction(protocol, raw_tx)` is callable and returns `Result<String, String>`

## Detailed Steps

1. Open `api/src/functions.rs`. Add:
   ```rust
   pub async fn broadcast_transaction(
       service: &paypunkd::services::PaypunkService,
       protocol: ProtocolId,
       raw_tx: Vec<u8>,
   ) -> Result<String, String> {
       service.broadcast_transaction(protocol, raw_tx).await
   }
   ```
   Add `ProtocolId` to the import from `paypunk_types` if not already there.

2. Open `api/src/client.rs`. Add:
   ```rust
   /// Broadcast a finalized, signed transaction to the network.
   pub async fn broadcast_transaction(
       &self,
       protocol: ProtocolId,
       raw_tx: Vec<u8>,
   ) -> Result<String, String> {
       crate::functions::broadcast_transaction(&self.service, protocol, raw_tx).await
   }
   ```
   Add `ProtocolId` to the import from `paypunk_types`.

3. Run `cargo build` and verify it compiles.

4. Run `cargo test` and verify all tests pass.

## Completion

When this step is complete — **STOP. DO NOT START THE NEXT STEP.**

```bash
git add -A && git commit -m "step 3: add broadcast to api crate"

mv todo/03_step.md done/

cat >> todo/goal.md << 'EOF'

## Step 3 — Done

Exposed `broadcast_transaction()` in the `paypunk-api` crate via `functions.rs` and `Client`.
EOF
```

## ⛔ STOP HERE. WAIT FOR INSTRUCTION. DO NOT READ OR EXECUTE STEP 4.
