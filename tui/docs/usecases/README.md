# TUI Wallet Usecases

| # | Usecase | File | Description |
|---|---------|------|-------------|
| 1 | [SetupScreen](01-setup.md) | `tui/src/screens/setup.rs:32` | Wallet creation/import wizard (7 sub-steps) |
| 2 | [GreetingScreen](02-greeting.md) | `tui/src/screens/greeting.rs:16` | Initial unlock prompt for existing wallet |
| 3 | [LockScreen](03-lock.md) | `tui/src/screens/lock.rs:17` | Re-authentication after auto-lock |
| 4 | [HomeScreen](04-home.md) | `tui/src/screens/home.rs:19` | Account list and main navigation |
| 5 | [AssetsScreen](05-assets.md) | `tui/src/screens/assets.rs:27` | Asset balance view with Send/Receive/History buttons |
| 6 | [SendScreen](06-send.md) | `tui/src/screens/send.rs:78` | Multi-step send flow (Form → Review → Sending → Confirm) |
| 7 | [ReceiveScreen](07-receive.md) | `tui/src/screens/receive.rs:15` | Display receiving address + QR code |
| 8 | [SettingsScreen](08-settings.md) | `tui/src/screens/settings.rs:21` | Auto-lock, fiat currency, reveal recovery phrase |
| 9 | [HelpScreen](09-help.md) | `tui/src/screens/help.rs:11` | Context-sensitive keybinding overlay |

## Architecture Layers

Each usecase flows through these layers:

```
TUI Screen  →  RealWalletApi  →  paypunk-api Client  →  IPC (Unix socket)
                                                              ↓
                                                          paypunkd
                                                              ↓
                                                          IPC (Unix socket)
                                                              ↓
                                                          keypunkd
```

- **TUI Screen**: Ratatui widget with `Screen` trait — renders UI and handles keyboard input
- **RealWalletApi**: `tui/src/api/real.rs` — implements `WalletApi` trait, communicates via `paypunk-api::Client`
- **paypunk-api Client**: `api/src/client.rs` — high-level client wrapping `PaypunkService` IPC calls
- **paypunkd**: `paypunkd/src/` — wallet daemon (DB, protocol implementations, orchestration)
- **keypunkd**: `keypunkd/src/` — key daemon (seed storage, signing, viewing key derivation)
