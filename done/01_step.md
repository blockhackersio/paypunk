# Step 1: Add `broadcast` to `Protocol` trait + implementations

## Description

Add a `broadcast()` method to the `Protocol` trait in `paypunk-types` so that signed/finalized transactions can be submitted to the network via the protocol implementation. Implement it for Ethereum (delegates to `EthRpcClient::send_raw_transaction`) and add a stub for Zcash.

## Files to modify

- `types/src/lib.rs` — Add `broadcast(&self, finalized_tx: &[u8]) -> Result<String, String>` to the `Protocol` trait
- `protocols/ethereum/src/protocol.rs` — Implement `broadcast` using `self.client.send_raw_transaction(finalized_tx)`
- `protocols/zcash/src/protocol.rs` — Add stub `broadcast` returning `Err("broadcast not yet implemented for Zcash")`

## Acceptance Criteria

- [ ] `cargo build` succeeds with no warnings
- [ ] `cargo test` passes all tests
- [ ] The `Protocol` trait has a `broadcast` method with a default impl or required impl
- [ ] `EthereumProtocol::broadcast` returns a tx hash string
- [ ] `ZcashProtocol::broadcast` returns an error

## Detailed Steps

1. Open `types/src/lib.rs`, find the `Protocol` trait. Add a new method:
   ```rust
   fn broadcast(&self, finalized_tx: &[u8]) -> Result<String, String>;
   ```
   This returns a transaction hash string on success.

2. Open `protocols/ethereum/src/protocol.rs`. Add this impl block within `impl<T: EthRpcClient> Protocol for EthereumProtocol<T>`:
   ```rust
   fn broadcast(&self, finalized_tx: &[u8]) -> Result<String, String> {
       self.client.send_raw_transaction(finalized_tx)
   }
   ```

3. Open `protocols/zcash/src/protocol.rs`. Add this impl block within `impl Protocol for ZcashProtocol`:
   ```rust
   fn broadcast(&self, _finalized_tx: &[u8]) -> Result<String, String> {
       Err("broadcast not yet implemented for Zcash — needs lightwalletd connection".to_string())
   }
   ```

4. Run `cargo build` and verify it compiles.

5. Run `cargo test` and verify all tests pass.

## Completion

When this step is complete — **STOP. DO NOT START THE NEXT STEP.**

```bash
# Commit all changes
git add -A && git commit -m "step 1: add broadcast to Protocol trait + implementations"

# Move step file to done
mv todo/01_step.md done/

# Append completion to goal.md
cat >> todo/goal.md << 'EOF'

## Step 1 — Done

Added `broadcast()` to `Protocol` trait. EthereumProtocol delegates to `send_raw_transaction`. ZcashProtocol returns an error stub.
EOF
```

## ⛔ STOP HERE. WAIT FOR INSTRUCTION. DO NOT READ OR EXECUTE STEP 2.
