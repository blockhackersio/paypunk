# Step 6: Implement transfer pipeline and remaining usecase stubs

## Issue
**#9** — Several paypunkd usecases are `todo!()` stubs:
- `create_transfer` (PCZT pipeline not implemented)
- `get_transaction_status` (needs lightwalletd/RPC client)
- `get_current_block_height` (needs lightwalletd/RPC client)
- `estimate_fee` (needs TransactionProposer)

The WalletDbActor already has `ProposeAndBuild` message handler (though it returns "not yet implemented"). The `LspClient` has `get_latest_height()`.

## What to do

1. **Implement `create_transfer()` in `paypunkd/src/usecases.rs`**:
   - Accept the WalletDbActor recipient
   - Send `WalletMessage::ProposeAndBuild` with the transfer details
   - Return the created PCZT bytes (the `ProposeAndBuild` handler in wallet_actor.rs needs to be completed to actually create a PCZT from the proposal using `pczt` crate)

2. **Complete `WalletMessage::ProposeAndBuild` handler** in `protocols/zcash/src/wallet_actor.rs`:
   - After `propose_standard_transfer_to_address` succeeds, convert the `TransactionProposal` to a PCZT
   - Use `pczt::Pczt` builder pattern or `zcash_client_backend`'s PCZT creation utilities
   - Return the serialized PCZT bytes

3. **Implement `estimate_fee()` in `paypunkd/src/usecases.rs`**:
   - Build a transfer proposal (same as create_transfer but without creating the PCZT)
   - Extract the fee from the proposal's `TransactionBalance`
   - Return the fee as a u64 in zatoshis

4. **Implement `get_current_block_height()` in `paypunkd/src/usecases.rs`**:
   - Accept the WalletDbActor recipient
   - Send a new `WalletMessage::GetBlockHeight` variant
   - Handler connects to lightwalletd via `LspClient::get_latest_height()` and returns the height
   - Add `GetBlockHeight` variant to `WalletMessage`

5. **Implement `get_transaction_status()` in `paypunkd/src/usecases.rs`**:
   - Accept the WalletDbActor recipient
   - Send a new `WalletMessage::GetTxStatus { txid: String }` variant
   - Handler queries the WalletDb for the transaction status
   - Return `TxStatus`

6. **Add corresponding IPC messages** in `paypunkd/src/messages.rs` for the new usecases.

7. **Wire handlers** in `paypunkd/src/paypunkd.rs`.

## Verification
- `cargo build` succeeds
- `cargo test` passes
- `create_transfer()` returns a valid PCZT when a synced wallet is available
- `estimate_fee()` returns a non-zero fee estimate
- `get_current_block_height()` returns a positive block height (or error if no lightwalletd)
- `get_transaction_status()` returns a status (or error for unknown txid)
- All functions return proper errors instead of panicking with `todo!()`
