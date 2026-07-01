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
| Sync | Manual trigger via `r` key in Assets screen, async with polling |
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
                → ZcashWalletClient::create_transaction_async()
                  → WalletDbActor (ProposeAndBuild)
                    → zcash_client_backend::propose_standard_transfer_to_address()
                    → zcash_client_backend::create_pczt_from_proposal()
                    → Returns unsigned PCZT bytes
              → KeypunkService::preview_artifact() [sends to keypunkd for signing preview]
```

### Sync Flow
```
TUI AssetsScreen (r key)
  → api.sync(protocol)
    → Client::sync(protocol)
      → paypunkd IPC: Sync { protocol }
        → Paypunkd::sync()
          → usecases::sync(protocol)
            → ZcashWalletClient::sync(fvk, birthday, lightwalletd_host)
              → WalletDbActor (Sync)
                → LspClient::scan_range(birthday, latest)
                  → zcash_client_backend::scan_block() for each block
                → Updates WalletDb in background
  → TUI polls GetSyncStatus every 2s
    → Shows progress in global status bar
```

## Files to Create

| # | File | Description |
|---|------|-------------|
| 1 | `protocols/zcash/src/lsp_client.rs` | Lightwalletd gRPC client for scanning and broadcast |
| 2 | `tui/src/screens/history.rs` | Generic transaction history screen |

## Files to Modify

| # | File | Changes |
|---|------|---------|
| 1 | `types/src/lib.rs` | Add `SyncStatus` struct |
| 2 | `config/src/lib.rs` | Add `lightwalletd_host` and `zcash_network` fields |
| 3 | `paypunkd/src/messages.rs` | Add `Sync`, `GetSyncStatus` request variants; extend `CreateAccount` with `birthday_height`; add response variants |
| 4 | `paypunkd/src/paypunkd.rs` | Handle `Sync`, `GetSyncStatus` requests |
| 5 | `paypunkd/src/usecases.rs` | Implement `sync()`, `get_sync_status()` usecases; update `create_account()` for birthday_height |
| 6 | `paypunkd/src/run.rs` | Create `WalletDb`, start `WalletDbActor`, inject into `ZcashProtocol`; add lightwalletd_host, zcash_network to Config |
| 7 | `paypunkd/src/config.rs` | Add `lightwalletd_host`, `zcash_network` to `ConfigSource` trait and impls |
| 8 | `paypunkd/Cargo.toml` | Add deps for `zcash_client_sqlite`, `rusqlite` (if not present) |
| 9 | `protocols/zcash/src/lib.rs` | Remove `#[cfg(feature = "wallet")]` gate on wallet_actor/wallet_client; add lsp_client module |
| 10 | `protocols/zcash/src/protocol.rs` | Add `wallet_client: Option<ZcashWalletClient>` field; implement `build()`, `get_balance()`, `broadcast()` |
| 11 | `protocols/zcash/src/wallet_actor.rs` | Implement `ProposeAndBuild` handler; add `Sync` and `GetStatus` message variants |
| 12 | `protocols/zcash/src/wallet_client.rs` | Add `sync()`, `get_status()` methods |
| 13 | `protocols/zcash/Cargo.toml` | Add `tonic`, move lightwalletd deps to regular deps |
| 14 | `api/src/client.rs` | Add `sync()`, `get_sync_status()` methods |
| 15 | `api/src/functions.rs` | Implement `sync()`, `get_sync_status()` functions |
| 16 | `tui/src/api/types.rs` | Add `SyncStatus` type |
| 17 | `tui/src/api/mod.rs` | Add `sync()`, `get_sync_status()` to `WalletApi` trait |
| 18 | `tui/src/api/mock.rs` | Implement mock `sync()`, `get_sync_status()` |
| 19 | `tui/src/api/real.rs` | Implement real `sync()`, `get_sync_status()`; update `submit_send_review()` to pass memo for Zcash |
| 20 | `tui/src/screens/assets.rs` | Add History button, sync status display, wire `r` key to trigger sync |
| 21 | `tui/src/screens/send.rs` | Add optional Memo field (shown for Zcash), ZEC amount validation |
| 22 | `tui/src/screens/setup.rs` | Add birthday height prompt for Zcash account creation |
| 23 | `tui/src/screens/mod.rs` | Register `HistoryScreen` module |
| 24 | `tui/src/screens/help.rs` | Add new keybindings |
| 25 | `tui/src/app.rs` | Add `Nav::History(account_id)` variant |
| 26 | `tui/src/lib.rs` | Add sync status bar rendering to main render loop |
| 27 | `cli/src/main.rs` | Add `--lightwalletd-host`, `--zcash-network` flags to `Paypunkd` subcommand; pass to paypunkd Config |

## Implementation Steps

### Step 1: Types + Config Foundation
- Add `SyncStatus` struct to `types/src/lib.rs`
- Add `lightwalletd_host` and `zcash_network` fields to `config/src/lib.rs`

### Step 2: IPC Messages
- Extend `PaypunkdRequest` with `Sync { protocol }`, `GetSyncStatus { protocol }`
- Extend `PaypunkdRequest::CreateAccount` with `birthday_height: Option<u64>`
- Add `PaypunkdResponse::SyncAck`, `PaypunkdResponse::SyncStatus(SyncStatus)`

### Step 3: API Crate
- Add `sync()` and `get_sync_status()` to `Client` and `functions.rs`

### Step 4: TUI API Layer
- Add `SyncStatus` to `tui/src/api/types.rs`
- Add `sync()` and `get_sync_status()` to `WalletApi` trait
- Implement in `MockWalletApi` (fake delay, return progress)
- Implement in `RealWalletApi` (proxy to Client)

### Step 5: LspClient Module
- Create `protocols/zcash/src/lsp_client.rs`
- `LspClient` struct wrapping tonic gRPC client to lightwalletd
- Methods: `scan_range()`, `broadcast_tx()`, `get_latest_height()`
- Update `protocols/zcash/Cargo.toml` with `tonic`, `lightwalletd-tonic` deps

### Step 6: WalletDbActor Implementation
- Implement `ProposeAndBuild` handler using `zcash_client_backend::propose_standard_transfer_to_address` + `create_pczt_from_proposal`
- Add `Sync { fvk, birthday_height, lightwalletd_host }` message variant
- Add `GetStatus` message variant returning `SyncStatus`
- Update `wallet_client.rs` with `sync()`, `get_status()` methods
- Remove `#[cfg(feature = "wallet")]` gates

### Step 7: ZcashProtocol Backend
- Add `wallet_client: Option<ZcashWalletClient>` field to `ZcashProtocol`
- Implement `build()` using wallet_client
- Implement `get_balance()` querying WalletDb for note sums
- Implement `broadcast()` using LspClient

### Step 8: paypunkd Wiring
- Update `run.rs` Config with `lightwalletd_host`, `zcash_network`
- Open Zcash WalletDb at `{data_dir}/zcash/{network}/wallet.db`
- Start `WalletDbActor`, inject into `ZcashProtocol`
- Update `ConfigSource` trait with new fields
- Handle `Sync` and `GetSyncStatus` in Paypunkd actor
- Implement `sync()` and `get_sync_status()` usecases
- Update `create_account()` usecase to pass birthday_height

### Step 9: TUI History Screen
- Create `tui/src/screens/history.rs`
- Minimal table: Date | Type (Sent/Received) | Amount | Status
- Sorted descending, no filtering
- Keybindings: Esc to go back

### Step 10: TUI Screen Modifications
- **Assets**: Add History button, sync status display, wire `r` key to `api.sync()`
- **Send**: Add optional Memo field (shown when protocol is Zcash), ZEC amount validation
- **Setup**: Add birthday height field during Zcash account creation
- **app.rs**: Add `Nav::History(account_id)` variant
- **lib.rs**: Add sync status bar to main render loop

### Step 11: CLI Flags
- Add `--lightwalletd-host` and `--zcash-network` to `Commands::Paypunkd` in `cli/src/main.rs`
- Pass through to `paypunkd::run::Config`
