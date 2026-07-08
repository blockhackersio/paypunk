# SPEC: Zcash in the TUI Wallet — Full Vertical Slice

## Overview

Add full Zcash support to the Paypunk TUI wallet: implement the missing `ZcashProtocol` backend methods (`build`, `get_balance`, `broadcast`), wire up a `WalletDbActor` with chain scanning via lightwalletd, and add TUI screens for transaction history, memo support, birthday height configuration, and sync status display.

## Architecture Decisions (from grill)

| Decision | Choice |
|----------|--------|
| Network | Both mainnet/testnet via `--zcash-network` flag |
| Lightwalletd | `--lightwalletd-host` CLI flag + TOML config, required for Zcash |
| WalletDb location | `{data_dir}/zcash/{network}/wallet.db` (managed by paypunkd) |
| Birthday height | Prompted in TUI during Zcash account creation, stored in WalletDb |
| Sync | Background loop (every 10s) via ScanActor, plus manual trigger |
| Sync status | Global status bar on all screens when syncing |
| Memo | Optional text field in send form, shown only for Zcash |
| History | Minimal table (date, type, amount, status), generic/protocol-agnostic |
| Shielding | Deferred to v2 |
| Send flow | Generic with minor Zcash-specific tweaks (memo field, ZEC validation) |
| Receive | Works as-is with Zcash UAs |
| WalletDbActor | Configured at protocol setup time in `run.rs`, injected into `ZcashProtocol` |
| WalletDb file | Separate file, not the encrypted paypunkd DB |

## Data Flow

### Send Flow (Zcash)
```
TUI SendScreen
  → api.submit_send_review()
    → RealWalletApi::submit_send_review()
      → Client::submit_intent(Intent::Zcash(ZcashIntent::Transfer { ... }))
        → paypunkd IPC: SubmitIntent
          → Paypunkd::submit_intent()
            → usecases::submit_intent()
              → Protocol::build() [ZcashProtocol]
                → WalletDbActor (ProposeAndBuild)
                  → zcash_client_backend::propose_standard_transfer_to_address()
                  → zcash_client_backend::create_pczt_from_proposal()
                  → Returns unsigned PCZT bytes
              → KeypunkService::preview_artifact() [sends to keypunkd for signing preview]
```

### Sync Flow
```
Background loop (every 10s in paypunkd run.rs)
  → ScanActor::handle(Sync)
    → LspClient::scan_range(birthday, latest)
      → WalletDbActor (ScanBlocks)
        → zcash_client_backend::scan_block() for each block
  → TUI polls GetSyncStatus every render tick (~50ms)
    → Shows progress in global status bar
```

## Files Created

| # | File | Description |
|---|------|-------------|
| 1 | `protocols/zcash/src/lsp_client.rs` | Lightwalletd gRPC client for scanning and broadcast |
| 2 | `tui/src/screens/history.rs` | Generic transaction history screen |

## Files Modified

| # | File | Changes |
|---|------|---------|
| 1 | `types/src/lib.rs` | `SyncStatus` struct added |
| 2 | `config/src/lib.rs` | `lightwalletd_host` and `zcash_network` fields added |
| 3 | `paypunkd/src/messages.rs` | `GetSyncStatus`, `CreateTransfer`, `EstimateFee`, `GetCurrentBlockHeight`, `GetTransactionStatus` variants added; `CreateAccount` extended with `birthday_height` |
| 4 | `paypunkd/src/paypunkd.rs` | Handle new request variants; background sync loop in `run.rs` |
| 5 | `paypunkd/src/usecases.rs` | Implement `get_sync_status`, `create_transfer`, `estimate_fee` usecases; `create_account` passes birthday_height |
| 6 | `paypunkd/src/run.rs` | Create `WalletDb`, start `WalletDbActor` + `ScanActor`, inject into `ZcashProtocol`; background sync loop |
| 7 | `paypunkd/src/config.rs` | `lightwalletd_host`, `zcash_network` in `ConfigSource` trait |
| 8 | `paypunkd/Cargo.toml` | Dependencies for `zcash_client_sqlite`, `rusqlite`, `tonic` |
| 9 | `protocols/zcash/src/lib.rs` | WalletDb init, `create_protocol()`, `ZcashStack`; `lsp_client` module |
| 10 | `protocols/zcash/src/protocol.rs` | `wallet_addr` field; implement `build()`, `get_balance()`, `broadcast()`, `create_transfer()`, `estimate_fee()`, `get_sync_status()`, `get_history()` |
| 11 | `protocols/zcash/src/wallet_actor.rs` | `ProposeAndBuild`, `Sync`, `GetStatus`, `GetBalance`, `GetHistory`, `RegisterAccount` handlers |
| 12 | `api/src/client.rs` | `get_sync_status()`, `get_history()` methods |
| 13 | `api/src/functions.rs` | `get_sync_status()`, `get_history()`, `create_account()` functions |
| 14 | `tui/src/api/types.rs` | `SyncStatus`, `HistoryRow`, `HistoryData` types |
| 15 | `tui/src/api/mod.rs` | `get_sync_status()`, `get_history()` added to `WalletApi` trait |
| 16 | `tui/src/api/mock.rs` | Mock implementations for sync and history |
| 17 | `tui/src/api/real.rs` | Real implementations for sync and history; memo support in `submit_send_review()` |
| 18 | `tui/src/screens/assets.rs` | History button, sync status display, `r` key triggers sync |
| 19 | `tui/src/screens/send.rs` | Optional Memo field (shown for Zcash), ZEC amount validation |
| 20 | `tui/src/screens/setup.rs` | Birthday height prompt for Zcash account creation |
| 21 | `tui/src/screens/mod.rs` | `HistoryScreen` module registered |
| 22 | `tui/src/screens/help.rs` | New keybindings for history and sync |
| 23 | `tui/src/app.rs` | `Nav::History(account_id)` variant; sync status polling in `tick()` |
| 24 | `tui/src/lib.rs` | Sync status bar rendering in main render loop |
| 25 | `cli/src/main.rs` | `--lightwalletd-host`, `--zcash-network` flags passed to paypunkd Config |

## Implementation Notes

- Sync is handled by a background loop in `paypunkd/src/run.rs` that sends `Sync` messages directly to the `ScanActor`, not through `PaypunkdRequest` IPC messages.
- The wallet client logic lives in `wallet_actor.rs` (not a separate `wallet_client.rs`).
- `CreateTransfer`, `EstimateFee`, `GetCurrentBlockHeight`, and `GetTransactionStatus` IPC messages were added instead of a single `Sync` message on `PaypunkdRequest`.
- The `SyncStatus` struct was added directly to `types/src/lib.rs`.
- `birthday_height` on `CreateAccount` was included from the start.
