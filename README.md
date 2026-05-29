# Paypunk

Shielded Zcash wallet infrastructure for desktop and agentic commerce.

Paypunk makes it easy to send, receive, and manage Zcash with strong privacy guarantees. It uses a blind light client approach — your view keys never leave the machine — so you get full chain awareness without trusting third parties with sensitive data.

## Architecture

Layered design that grows with your needs:

- **Wallet API** — Core library for shielded (sapling) Zcash operations
- **CLI** — Scriptable command-line interface wrapping the wallet API
- **TUI** — Terminal-based interactive UI (planned)
- **Tauri desktop** — Native desktop app (future)

### Dual-daemon design

Two components with a strict security boundary:

- **Key Daemon** — Holds decrypted keys in memory. Only accepts sign/prove requests, never exposes raw keys.
- **Wallet Daemon** — Manages addresses, chain sync via blind LSP, balance tracking, and transaction construction. Delegates signing to the Key Daemon.

## Privacy

- Sapling shielded pool only — all transactions are private
- Blind LSP scanning — view keys never leave your machine, only diversifier prefixes are sent to public servers
- Seed encrypted at rest with Argon2id-derived key
- Separate encryption for wallet state database
- One-time addresses — each incoming payment gets a new address, never reused

## Getting started

*Coming soon.* Build from source once the initial wallet API is functional.
