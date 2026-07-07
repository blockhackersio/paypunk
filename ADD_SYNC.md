# Background Continuous Sync

## Goal

Sync occurs continuously in the background while paypunkd is running — every new block is fetched and scanned automatically.

## Current State

- Sync is on-demand only (unlock, create_account, broadcast)
- `WalletMessage::Sync` carries `fvk`, `birthday_height`, `lightwalletd_host` on every call
- `try_sync()` is monolithic: FVK parsing, account registration, tree state, block fetch, scan — all in one function
- Three overlapping entry points on `Protocol` trait: `start_background_sync`, `sync_account`, `trigger_sync`
- `sync_wallet()` usecase is a `todo!()`
- scan_queue priority SQL hack after every sync
- No background polling loop

## Changes

### 1. Store state in WalletDbActor

**File**: `protocols/zcash/src/wallet_actor.rs`

Add fields:
- `lightwalletd_host: String` — passed at construction, never changes
- `accounts: Vec<(Vec<u8>, u64)>` — registered (FVK, birthday) pairs

### 2. Simplify WalletMessage enum

Replace:
- `Sync { fvk, birthday_height, lightwalletd_host }` — removed
- `GetBlockHeight { lightwalletd_host }` — removed (unused in send flow)

With:
- `RegisterAccount { fvk: Vec<u8>, birthday_height: u64 }` — one-time setup: parse FVK, get tree state, import into WalletDb, do initial full sync from birthday, store in `self.accounts`
- `Sync` — no params. Incremental sync from current chain tip using stored accounts. No-op if no accounts registered.

### 3. Refactor try_sync → register_account + sync_from_tip

- `register_account(fvk, birthday_height)` — one-time: parse FVK, get tree state, import into WalletDb, store in self.accounts and fvk_to_account_id, then do initial full sync from birthday to tip
- `sync_from_tip()` — incremental: iterate stored accounts, get latest height from lightwalletd, fetch only blocks from chain_tip+1 to latest, scan, update chain tip

### 4. Remove scan_queue SQL hack

Delete lines 308-314 from wallet_actor.rs. Incremental sync means `scan_cached_blocks` handles scan queue correctly.

### 5. Simplify Protocol trait

**File**: `types/src/lib.rs`

Remove:
- `async fn start_background_sync(&self, _accounts: &[Account])` — trait method removed
- `async fn trigger_sync(&self)` — trait method removed (default no-op)

Add:
- `async fn register_account(&self, viewing_key: &[u8], birthday_height: u64, address: &str) -> Result<(), String>` — replaces both `sync_account` and `start_background_sync`

Keep:
- `async fn sync_account(...)` — but rename to `register_account` (or keep as alias that calls register_account)
- `async fn get_sync_status(&self)` — unchanged

Actually, keep the existing `sync_account` name on the trait since it's already used by paypunkd. Just change its semantics to register-only (no sync), and the background loop handles the actual syncing.

### 6. Update ZcashProtocol

**File**: `protocols/zcash/src/protocol.rs`

- `sync_account()` → sends `RegisterAccount` message (no sync, just registration + initial full sync)
- Remove `start_background_sync()` — no longer on trait
- `trigger_sync()` → sends parameterless `Sync` message
- Add `pub fn wallet_recipient(&self) -> Option<Recipient<WalletMessage>>` — exposes recipient for background loop
- Remove `sync_via_wallet()` — no longer needed (replaced by direct message sends in the methods above)

### 7. Update paypunkd flows

**File**: `paypunkd/src/paypunkd.rs`

- `unlock()`: replace `proto.start_background_sync(&accounts)` with `proto.sync_account(...)` for each account (which now does registration + initial full sync)
- `create_account()`: `proto.sync_account(...)` stays the same (it now does registration + initial full sync)

**File**: `paypunkd/src/usecases.rs`

- `broadcast_transaction()`: remove `protocols.get(protocol)?.trigger_sync().await` — background loop handles it
- `sync_wallet()`: replace `todo!()` with send `Sync` message via protocol

### 8. Add background sync loop

**File**: `paypunkd/src/run.rs`

After protocols are registered and paypunkd actor is started, spawn:

```rust
// Background sync loop
if let Ok(zcash) = protocols.get(ProtocolId::Zcash) {
    if let Some(recipient) = zcash.wallet_recipient() {
        let interval_secs = /* configurable, default 10 */;
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
            loop {
                interval.tick().await;
                let _ = recipient.ask(WalletMessage::Sync).await;
            }
        });
    }
}
```

### 9. Remove unused GetBlockHeight

**File**: `protocols/zcash/src/wallet_actor.rs`

Remove `WalletMessage::GetBlockHeight` variant and its handler (lines ~661-667). It's not used in the send flow and the background loop replaces any need for it.

Also remove `get_current_block_height()` from the `Protocol` trait if nothing calls it. Check callers first.

## Files Changed

| File | Change |
|------|--------|
| `protocols/zcash/src/wallet_actor.rs` | Core: new fields, refactored messages, split sync logic, remove SQL hack |
| `protocols/zcash/src/protocol.rs` | Simplify: remove start_background_sync/trigger_sync, add wallet_recipient accessor |
| `protocols/zcash/src/lib.rs` | Pass lightwalletd_host to WalletDbActor::new() |
| `types/src/lib.rs` | Remove start_background_sync/trigger_sync from Protocol trait |
| `paypunkd/src/run.rs` | Add background sync loop |
| `paypunkd/src/paypunkd.rs` | Update unlock/create_account to use new flow |
| `paypunkd/src/usecases.rs` | Implement sync_wallet, remove trigger_sync after broadcast |

## Verification

1. `cargo build` in workspace
2. `cargo clippy` — no new warnings
3. Integration test: start paypunkd, unlock, observe sync logs appearing every ~10s
4. On regtest: mine blocks, verify they're picked up by background sync within one interval
