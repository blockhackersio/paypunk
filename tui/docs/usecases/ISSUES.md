# TUI Wallet — Known Issues

## 1. Address Book Not Persisted
**File:** `tui/src/api/real.rs:24`
**Severity:** Medium

`RealWalletApi` stores address book entries in an in-memory `Mutex<Vec<AddressBookEntry>>`. Entries added during send (`submit_send_confirm` at line 451-457) are lost on restart. No IPC messages exist for address book CRUD with paypunkd, and no SQLite table exists for it.

## 2. `submit_lock()` is a No-Op
**File:** `tui/src/api/real.rs:523-525`
**Severity:** High

`RealWalletApi::submit_lock()` always returns `Ok(())` regardless of the password provided. There is no actual authentication check — any password unlocks the wallet. The lock screen is purely cosmetic.

## 3. `submit_settings()` is a No-Op
**File:** `tui/src/api/real.rs:537-539`
**Severity:** Low

Settings changes (auto-lock timeout, fiat currency) are accepted but never persisted. `get_settings()` always returns hardcoded values (`auto_lock_minutes: 5`, `fiat_currency: "USD"`). No IPC or DB write occurs.

## 4. `submit_reveal_phrase()` Not Implemented for Real API
**File:** `tui/src/api/real.rs:541-548`
**Severity:** Medium

Returns `Err("reveal phrase not yet supported via real API")`. The full IPC chain through paypunkd → keypunkd → `seed.enc` decrypt → mnemonic export is not wired. Users cannot view their recovery phrase in production. Mock API returns hardcoded words.

## 5. `sync()` and `get_sync_status()` Are Stubs
**File:** `paypunkd/src/usecases.rs:112-140`
**Severity:** High

`sync()` logs "sync requested for Zcash" but performs no actual chain synchronization. `get_sync_status()` always returns `SyncStatus { is_syncing: false, current_height: 0, target_height: 0 }`. The TUI renders a sync progress bar but it will never show activity.

## 6. `HistoryScreen` is a Placeholder
**File:** `tui/src/screens/history.rs:22`
**Severity:** Medium

Shows "No transactions yet" with TODO comments. `paypunkd/src/usecases.rs::get_history()` is `todo!("get_history: needs Page/HistoryEntry types")`. No transaction history is available.

## 7. AssetsScreen Balance Stale After Send
**File:** `tui/src/screens/assets.rs:71-80`
**Severity:** Low

After sending a transaction and popping back to AssetsScreen, `on_reactivate` re-fetches assets via chain RPC. The balance may show the pre-send value due to RPC propagation delay. No optimistic local balance deduction is applied.

## 8. Send Confirm Depends on `tick()` Timing
**File:** `tui/src/screens/send.rs:156-173`
**Severity:** Low

When the user presses Enter on the Review step, `handle_input` sets `step = Sending` and stores pending data. The actual `submit_send_confirm()` call happens in the next `tick()` invocation (~50ms later via resize events). If `tick()` is somehow skipped, the send stalls indefinitely with no user feedback.

## 9. Several paypunkd Usecases Are Stubs
**File:** `paypunkd/src/usecases.rs`
**Severity:** Medium

| Function | Line | Status |
|----------|------|--------|
| `create_transfer` | 353 | `todo!()` — PCZT pipeline not implemented |
| `get_history` | 366 | `todo!()` — needs Page/HistoryEntry types |
| `sync_wallet` | 376 | `todo!()` |
| `get_transaction_status` | 393 | `todo!()` — needs lightwalletd/RPC client |
| `get_current_block_height` | 402 | `todo!()` — needs lightwalletd/RPC client |
| `estimate_fee` | 410 | `todo!()` — needs TransactionProposer |

## 10. `get_lock()` Returns Hardcoded Data
**File:** `tui/src/api/real.rs:514-521`
**Severity:** Low

Always returns `LockData { auth_methods: { password_set: true }, failed_attempts: 0 }` regardless of actual state. No IPC or DB read occurs.

## 11. `get_settings()` Returns Hardcoded Data
**File:** `tui/src/api/real.rs:527-535`
**Severity:** Low

Always returns `SettingsData { security: { auto_lock_minutes: 5 }, fiat_currency: "USD", app_version: "0.1.0" }`. Never reads from config file or DB.
