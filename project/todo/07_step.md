# Step 7: TUI polish — stale balance and tick timing fixes

## Issue
**#7** — After sending a transaction and popping back to `AssetsScreen`, `on_reactivate` re-fetches assets via chain RPC. The balance may show the pre-send value due to RPC propagation delay. No optimistic local balance deduction is applied.

**#8** — When the user presses Enter on the Review step in `SendScreen`, `handle_input` sets `step = Sending` and stores pending data. The actual `submit_send_confirm()` call happens in the next `tick()` invocation (~50ms later via resize events). If `tick()` is somehow skipped, the send stalls indefinitely with no user feedback.

## What to do

1. **Fix optimistic balance deduction** (`tui/src/screens/assets.rs` and `tui/src/api/real.rs`):
   - In `AssetsScreen::on_reactivate()`, after re-fetching from the API, apply an optimistic deduction
   - Store the last sent amount and address in the `AssetsScreen` (passed back when returning from SendScreen)
   - Deduct the sent amount from the balance display immediately, even before the RPC confirms
   - Add a visual indicator (e.g., "(pending)" suffix) to show the balance is awaiting confirmation

2. **Fix send tick() dependency** (`tui/src/screens/send.rs`):
   - Move the `submit_send_confirm()` call from `tick()` into `handle_input()` directly
   - When user presses Enter on Review step, make the IPC call immediately in `handle_input` instead of deferring to `tick()`
   - Show a spinner/sending state while the IPC is in-flight
   - Handle the result and transition to Confirm step synchronously within the input handler
   - Remove the `PendingSend` struct and `tick()`-based submission logic

3. **Add feedback mechanism** — if the IPC call takes long, ensure the UI still renders the "Broadcasting" spinner during the await (the async call in handle_input should yield to the runtime, allowing renders to happen).

## Verification
- `cargo build` succeeds
- `cargo test` passes
- After sending, balance immediately shows the deducted amount with a "(pending)" indicator
- The send no longer depends on `tick()` firing — pressing Enter immediately initiates the broadcast
- If `tick()` is never called, the send still completes
