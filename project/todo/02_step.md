# Step 2: Implement transaction history in paypunkd and wire HistoryScreen

## Issue
**#6** — `HistoryScreen` shows "No transactions yet" with TODO comments. Never loads real data.
**#9 (partial)** — `get_history()` in `paypunkd/src/usecases.rs` is a `todo!("get_history: needs Page/HistoryEntry types")` stub.

The types `Page<T>`, `HistoryEntry`, `TxDirection`, `TxStatus` already exist in `paypunk-types` (`types/src/lib.rs`).

## What to do

1. **Add IPC messages** in `paypunkd/src/messages.rs`:
   - `GetHistory { protocol: ProtocolId, account_id: u32, cursor: Option<String>, limit: u32 }`
   - Response: `HistoryResult { entries: Vec<HistoryEntry>, next_cursor: Option<String>, has_more: bool }`

2. **Implement `get_history()` in `paypunkd/src/usecases.rs`**:
   - Accept the WalletDbActor recipient (similar to sync)
   - Send a `WalletMessage` to query transaction history from the WalletDb
   - Return `Page<HistoryEntry>` with real data
   - For now, if no WalletDbActor is registered, return an empty page (not a todo panic)

3. **Add a handler** in `paypunkd/src/paypunkd.rs` for `GetHistory`.

4. **Add IPC message** in `protocols/zcash/src/wallet_actor.rs`:
   - Add a `GetHistory { account: u32, cursor: Option<String>, limit: u32 }` variant to `WalletMessage`
   - Implement it by querying `zcash_client_sqlite::WalletDb` for sent/received transactions and mapping to `HistoryEntry`

5. **Wire `HistoryScreen`** in `tui/src/screens/history.rs`:
   - In `init()` and `on_reactivate()`, call `api.get_history()` (you may need to add this to the `WalletApi` trait)
   - Populate `self.rows` from the returned entries
   - Remove the hardcoded empty-state TODO comment

6. **Add `get_history()` to the `WalletApi` trait** in `tui/src/api/mod.rs` and implement it in `tui/src/api/real.rs`:
   - Call the new IPC `GetHistory` message via `self.client`
   - Map results to `HistoryRow` structs for display

## Verification
- `cargo build` succeeds
- `cargo test` passes
- `HistoryScreen` renders real transaction data when a synced wallet is available
- Returns empty state gracefully when no wallet/sync has occurred
