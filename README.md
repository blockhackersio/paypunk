# Paypunk Wallet Toolkit

_This is experimental software and should not be used with real funds_

[![CI](https://github.com/blockhackersio/paypunk/actions/workflows/ci.yml/badge.svg)](https://github.com/blockhackersio/paypunk/actions/workflows/ci.yml)
[![License: AGPL-3.0](https://img.shields.io/badge/license-AGPL--3.0-blue.svg)](https://www.gnu.org/licenses/agpl-3.0)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org/)
[![Status](https://img.shields.io/badge/status-alpha-yellow.svg)]()

Flexible wallet infrastructure for privacy-preserving commerce.

## Architecture

Layered, multi-process design:

- **`types`** — Chain-agnostic domain types (`Address`, `Amount`, `Balance`, `Transfer`, `Intent`, `Protocol`/`SignerProtocol` traits, etc.). No chain-specific logic.
- **`config`** — TOML-based configuration with environment variable overrides (socket paths, data directory, RPC endpoints, network selection).
- **`api`** — Chain-agnostic library. Dispatches to the appropriate chain backend by `ProtocolId` (Zcash, Ethereum). Hides IPC and actor details from consumers.
- **`paypunkd`** — App daemon (library crate, launched via `paypunk paypunkd`). Hosts the `Paypunkd` actor, usecases, service orchestration, chain backend injection.
- **`keypunkd`** — Key daemon (library crate, launched via `paypunk keypunkd`). Hosts the `Keypunkd` actor. Seed generation, signing, proving. Designed to run as a separate system user (deployment concern, not enforced by code).
- **`ipc`** — Tactix actor sender for interprocess communication over Unix sockets. Carries opaque byte payloads; serialization (postcard) is done by callers.
- **`protocols/{zcash,ethereum}`** — Chain-specific implementations of the `Protocol` and `SignerProtocol` traits from `paypunk-types`.
- **`cli`** — Command-line interface binary (`paypunk`). Uses `api` for scripting and automation. Also launches daemons and the TUI.
- **`tui`** — Terminal-based interactive UI (ratatui). Library crate consumed by the CLI, also builds as a standalone binary.
- **`bridge`** — WebSocket/HTTP relay between a local IPC client and a browser, for air-gapped QR-based signing.
- **`signer`** — Tauri v2 mobile app for offline air-gapped signing (separate build, excluded from workspace).
- **`ping`/`pong`** — Diagnostic IPC round-trip test pair.

### Process Model

Three processes with a strict security boundary:

- **paypunk** — CLI/TUI binary. Connects to paypunkd via the `api` library. Never touches key material directly.
- **paypunkd** — Manages addresses, chain sync, balance tracking, and transfer construction. Delegates signing to keypunkd. Never holds key material.
- **keypunkd** — Holds decrypted keys in protected memory. Accepts sign/prove requests from any process that completes the X25519 IPC handshake, never exposes raw key material.

## Privacy

- Zcash Orchard shielded pool and Ethereum support
- Seed encrypted at rest with Argon2id-derived key (AES-256-GCM)
- Wallet state database is currently plaintext (`paypunkd.db`); encryption at rest is planned

## Getting started

The `paypunk` binary auto-launches both daemons and the TUI when run with no subcommand. Individual subcommands are also available:

```bash
cargo run --                   # auto-launch keypunkd + paypunkd + TUI
cargo run -- keypunkd          # launch key daemon only
cargo run -- paypunkd          # launch app daemon only
cargo run -- tui               # launch TUI only (daemons must be running)
cargo run -- generate-seed -p <password>   # CLI: generate a new wallet
cargo run -- get-balance --protocol zcash  # CLI: check balance
```
