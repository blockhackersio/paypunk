# Goal: Wire TUI → API for Ethereum Send

## What

Make the paypunk TUI capable of sending Ethereum transactions end-to-end through the full architecture:

```
TUI (ratatui) → api crate → paypunkd (app daemon) → keypunkd (key daemon)
```

## Current State

- The **entire backend pipeline** works: `EthereumProtocol::build()` constructs EIP-1559 txs, `keypunkd` signs them, `HttpRpcClient::send_raw_transaction()` broadcasts them
- The **api crate** exposes `submit_intent()`, `approve_signature()`, `get_balance()` etc.
- The **TUI** uses `MockWalletApi` exclusively — no real connection to paypunkd
- There is **no broadcast path** through the daemon messages (signed txs can't be submitted to the network via paypunkd)
- The **Ethereum nonce is hardcoded to 0** instead of fetched from chain
- The **TUI event loop is synchronous** but `api::Client` is async

## What Needs to Happen

### Backend additions
1. Add `broadcast()` method to the `Protocol` trait and implement it for Ethereum (Zcash gets a stub)
2. Add `BroadcastTransaction` to paypunkd request/response messages with handler, usecase, and service method
3. Expose `broadcast_transaction()` in the api crate

### TUI async migration
4. Make `WalletApi` trait async using `async-trait`
5. Make `Screen` trait async, update all screens and `App`
6. Refactor the TUI event loop to run on tokio with async event handling

### TUI real backend integration
7. Create `RealWalletApi` that wraps `api::Client` and implements the two-phase send flow
8. Wire CLI socket path through to TUI, select real vs mock backend

### Fixes + tests
9. Fix hardcoded `nonce = 0` in Ethereum protocol to use `get_transaction_count()`
10. Add end-to-end integration test for Ethereum send (submit_intent → approve_signature → broadcast)

## Acceptance Criteria

- A user can launch `paypunk tui --socket-path /tmp/paypunkd.sock` with a running paypunkd+keypunkd stack
- Navigate to an Ethereum asset, tap Send, enter a recipient and amount, review, confirm
- The transaction is built, signed by keypunkd, and broadcast to the configured Ethereum RPC
- The TUI shows the confirmed transaction hash and block explorer URL
- All existing tests continue to pass
- The code compiles without warnings

## Step 1 — Done

Added `broadcast()` to `Protocol` trait. EthereumProtocol delegates to `send_raw_transaction`. ZcashProtocol returns an error stub.

## Step 2 — Done

Added `BroadcastTransaction`/`TransactionBroadcasted` to paypunkd messages. Implemented handler, usecase (finalize + broadcast via protocol), and PaypunkService method.

## Step 3 — Done

Exposed `broadcast_transaction()` in the `paypunk-api` crate via `functions.rs` and `Client`.
