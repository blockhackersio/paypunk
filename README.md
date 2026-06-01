# Paypunk

> 'Private money, punk.'

Wallet infrastructure for privacy-preserving commerce on desktop and agentic workflows.

## Architecture

Layered, multi-process design:

- **`api`** — Chain-agnostic library. Accepts an asset type (Zcash, Ethereum) and dispatches to the appropriate chain backend. Hides IPC and actor details from consumers.
- **`paypunkd`** — App daemon. Hosts WalletActor, usecases, service orchestration, chain backend injection.
- **`keypunkd`** — Key daemon. Hosts KeyActor. Seed generation, signing, proving. Runs as a separate system user.
- **`ipc`** — Tactix actor sender for interprocess communication over Unix sockets.
- **`cli`** — Command-line interface using `api` for scripting and automation.
- **`tui`** — Terminal-based interactive UI (ratatui).

### Process model

Three processes with a strict security boundary:

- **paypunk** — CLI/TUI binary. Connects to paypunkd via the `api` library. Never touches key material directly.
- **paypunkd** — Manages addresses, chain sync, balance tracking, and transfer construction. Delegates signing to keypunkd. Never holds key material.
- **keypunkd** — Holds decrypted keys in protected memory. Only accepts sign/prove requests from paypunkd, never exposes raw key material.

## Privacy

- All shielded pools: Sapling and Orchard
- Blind LSP scanning — view keys never leave your machine, only diversifier prefixes are sent to public servers
- Seed encrypted at rest with Argon2id-derived key
- Separate encryption for wallet state database via HKDF-split key

## Getting started

*Coming soon.* Build from source once the initial `api` is functional.
