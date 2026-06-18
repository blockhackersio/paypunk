# Step 2: Add broadcast to paypunkd messages + handlers + services

## Description

Add `BroadcastTransaction` to the paypunkd request/response message enums, implement the handler in the actor, implement the usecase, and expose it through the `PaypunkService`.

## Files to modify

- `paypunkd/src/messages.rs` — Add `BroadcastTransaction` request variant and `TransactionBroadcasted` response variant
- `paypunkd/src/usecases.rs` — Implement `broadcast_transaction` function (replacing the `todo!()` stub)
- `paypunkd/src/paypunkd.rs` — Add handler method for `BroadcastTransaction` that calls `Protocol::finalize` then `Protocol::broadcast`
- `paypunkd/src/services.rs` — Add `broadcast_transaction` method to `PaypunkService`

## Acceptance Criteria

- [ ] `cargo build` succeeds with no warnings
- [ ] `cargo test` passes all tests
- [ ] `PaypunkdRequest::BroadcastTransaction { protocol: ProtocolId, raw_tx: Vec<u8> }` exists
- [ ] `PaypunkdResponse::TransactionBroadcasted { tx_hash: String }` exists
- [ ] `PaypunkService::broadcast_transaction()` can be called from the api crate

## Detailed Steps

1. Open `paypunkd/src/messages.rs`. Add to `PaypunkdRequest`:
   ```rust
   BroadcastTransaction {
       protocol: ProtocolId,
       raw_tx: Vec<u8>,
   },
   ```
   Add to `PaypunkdResponse`:
   ```rust
   TransactionBroadcasted { tx_hash: String },
   ```

2. Open `paypunkd/src/usecases.rs`. Replace the `todo!()` stub:
   ```rust
   pub fn broadcast_transaction(
       protocols: &ProtocolService,
       protocol: ProtocolId,
       raw_tx: Vec<u8>,
   ) -> Result<String, String> {
       let finalized = protocols.get(protocol)?.finalize(&raw_tx)?;
       protocols.get(protocol)?.broadcast(&finalized)
   }
   ```

3. Open `paypunkd/src/paypunkd.rs`. Add a handler method:
   ```rust
   async fn broadcast_transaction(&self, protocol: ProtocolId, raw_tx: Vec<u8>) -> PaypunkdResponse {
       info!(?protocol, "broadcasting transaction");
       self.respond(
           "broadcast_transaction",
           usecases::broadcast_transaction(&self.protocols, protocol, raw_tx),
           |tx_hash| PaypunkdResponse::TransactionBroadcasted { tx_hash },
       )
   }
   ```
   Then add the match arm in the `Handler<IpcMessage>` impl:
   ```rust
   PaypunkdRequest::BroadcastTransaction { protocol, raw_tx } => {
       self.broadcast_transaction(protocol, raw_tx).await
   }
   ```

4. Open `paypunkd/src/services.rs`. Add:
   ```rust
   pub async fn broadcast_transaction(
       &self,
       protocol: ProtocolId,
       raw_tx: Vec<u8>,
   ) -> Result<String, String> {
       match self
           .send(PaypunkdRequest::BroadcastTransaction { protocol, raw_tx })
           .await?
       {
           PaypunkdResponse::TransactionBroadcasted { tx_hash } => Ok(tx_hash),
           PaypunkdResponse::Error { message } => Err(message),
           _ => Err("unexpected response variant".to_string()),
       }
   }
   ```

5. Run `cargo build` and verify it compiles.

6. Run `cargo test` and verify all tests pass.

## Completion

When this step is complete — **STOP. DO NOT START THE NEXT STEP.**

```bash
git add -A && git commit -m "step 2: add broadcast to paypunkd messages + handlers + services"

mv todo/02_step.md done/

cat >> todo/goal.md << 'EOF'

## Step 2 — Done

Added `BroadcastTransaction`/`TransactionBroadcasted` to paypunkd messages. Implemented handler, usecase (finalize + broadcast via protocol), and PaypunkService method.
EOF
```

## ⛔ STOP HERE. WAIT FOR INSTRUCTION. DO NOT READ OR EXECUTE STEP 3.
