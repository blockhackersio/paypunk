# Step 1: Wire Zcash WalletDbActor into paypunkd sync pipeline

## Issue
**#5** — `sync()` and `get_sync_status()` in `paypunkd/src/usecases.rs` are stubs that log but perform no actual chain sync.
**#9 (partial)** — `sync_wallet()` is a `todo!()` stub.

The `WalletDbActor` (in `protocols/zcash/src/wallet_actor.rs`) already exists with a `Sync` handler that connects to lightwalletd, scans blocks, and tracks progress via atomics. However, it is not wired into paypunkd's request handling.

## What to do

1. **Add an IPC message** in `paypunkd/src/messages.rs` for registering a Zcash FVK with birthday height:
   - `RegisterZcashWallet { fvk: Vec<u8>, birthday_height: u64, lightwalletd_host: String }`
   - Response: `ZcashWalletRegistered`

2. **Store the WalletDbActor recipient** in the `Paypunkd` actor struct (use `Option<Recipient<WalletMessage>>`).

3. **Modify `sync()` in `usecases.rs`** to send a `WalletMessage::Sync` to the WalletDbActor instead of just logging. Accept the `recipient` as a parameter.

4. **Modify `get_sync_status()` in `usecases.rs`** to send a `WalletMessage::GetStatus` to the WalletDbActor and return the real `SyncStatus`.

5. **Wire the handlers** in `paypunkd/src/paypunkd.rs` for the new `RegisterZcashWallet` message and update the existing `Sync`/`GetSyncStatus` handlers to pass the WalletDbActor recipient.

6. **Create a `ZcashWalletClient`** in `protocols/zcash/src/wallet_client.rs` that wraps the `Recipient<WalletMessage>` (similar pattern to `PaypunkService` in `paypunkd/src/services.rs`). This is already declared as a module but may need filling in.

7. **Wire in `paypunkd` startup** (likely in `run.rs` or `paypunkd.rs`): when the Zcash protocol is registered, create the `WalletDbActor`, spawn it, and store its recipient.

## Verification
- `cargo build` succeeds
- `cargo test` passes
- Sync IPC round-trip works: calling `sync()` via IPC triggers a real lightwalletd connection attempt (will fail if no lightwalletd running, but returns an error instead of silently succeeding)
- `get_sync_status()` returns real status from WalletDbActor atomics instead of hardcoded zeros
