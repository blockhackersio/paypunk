# TUI Wallet Usecases

| # | Usecase | File | Description | Persistence |
|---|---------|------|-------------|-------------|
| 1 | [SetupScreen](01-setup.md) | `tui/src/screens/setup.rs:32` | Wallet creation/import wizard (7 sub-steps) | Writes `seed.enc` (keypunkd), writes `pre_derived_keys` + `accounts` tables (paypunkd) |
| 2 | [GreetingScreen](02-greeting.md) | `tui/src/screens/greeting.rs:16` | Initial unlock prompt for existing wallet | Reads `seed.enc` + `paypunkd.db.enc`, writes `pre_derived_keys` + `accounts` tables |
| 3 | [LockScreen](03-lock.md) | `tui/src/screens/lock.rs:17` | Re-authentication after auto-lock | None — no-op implementation |
| 4 | [HomeScreen](04-home.md) | `tui/src/screens/home.rs:19` | Account list and main navigation | Reads `accounts` table, writes `accounts` on add |
| 5 | [AssetsScreen](05-assets.md) | `tui/src/screens/assets.rs:27` | Asset balance view with Send/Receive/History buttons | Reads `accounts` table; balance from chain RPC |
| 6 | [SendScreen](06-send.md) | `tui/src/screens/send.rs:78` | Multi-step send flow (Form → Review → Sending → Confirm) | Reads `accounts` table; address book persisted to SQLite; signing reads `seed.enc` |
| 7 | [ReceiveScreen](07-receive.md) | `tui/src/screens/receive.rs:15` | Display receiving address + QR code | Reads `accounts` table |
| 8 | [SettingsScreen](08-settings.md) | `tui/src/screens/settings.rs:21` | Auto-lock, fiat currency, reveal recovery phrase | Reads/writes `settings` table; reveal reads `seed.enc` via keypunkd |
| 9 | [HelpScreen](09-help.md) | `tui/src/screens/help.rs:11` | Context-sensitive keybinding overlay | None — pure UI overlay |

## Persistence Layer Summary

### keypunkd — Seed Store
- **File:** `{data_dir}/seed.enc`
- **Format:** `[salt(16B) | nonce(12B) | AES-256-GCM ciphertext]`
- **Encryption:** Argon2id key derivation + AES-256-GCM
- **Access:** Atomic write via `seed.enc.tmp` + rename; read via `std::fs::read`

### paypunkd — SQLite Database
- **File (at rest):** `{data_dir}/paypunkd.db.enc`
- **File (decrypted):** `{data_dir}/paypunkd.db` (plaintext while unlocked)
- **Encryption:** Argon2id + AES-256-GCM (separate from seed encryption)
- **Tables:**
  - `accounts` — `id TEXT PK, protocol TEXT, derivation_path TEXT, name TEXT, address TEXT, viewing_key BLOB, created_at INTEGER`
  - `pre_derived_keys` — `protocol TEXT, account_index INTEGER, viewing_key BLOB, created_at INTEGER` (composite PK)
  - `_migrations` — migration tracking
- **Lock/unlock:** DB is encrypted at rest; decrypted to temp file on unlock; re-encrypted on close

### Address Book
- **Persisted** in paypunkd SQLite `address_book` table via `AddAddressBookEntry`/`GetAddressBook` IPC messages

## Architecture Layers

```
TUI Screen  →  RealWalletApi  →  paypunk-api Client  →  IPC (Unix socket)
                                                              ↓
                                                          paypunkd
                                                         ↙        ↘
                                                   SQLite DB    keypunkd (IPC)
                                                                    ↓
                                                               seed.enc (disk)
```

- **TUI Screen**: Ratatui widget with `Screen` trait — renders UI and handles keyboard input
- **RealWalletApi**: `tui/src/api/real.rs` — implements `WalletApi` trait, communicates via `paypunk-api::Client`
- **paypunk-api Client**: `api/src/client.rs` — high-level client wrapping `PaypunkService` IPC calls
- **paypunkd**: `paypunkd/src/` — wallet daemon (DB, protocol implementations, orchestration)
- **keypunkd**: `keypunkd/src/` — key daemon (seed storage, signing, viewing key derivation)
