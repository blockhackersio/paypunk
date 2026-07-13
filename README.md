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

## Installation

### From GitHub

```bash
cargo install --git https://github.com/blockhackersio/paypunk
```

### From source

```bash
git clone https://github.com/blockhackersio/paypunk.git
cd paypunk
cargo install --path cli
```

The `paypunk` binary is installed to `~/.cargo/bin/paypunk`.

## Getting started

The `paypunk` binary auto-launches both daemons and the TUI when run with no subcommand. Individual subcommands are also available:

```bash
paypunk                                # auto-launch keypunkd + paypunkd + TUI
paypunk keypunkd                       # launch key daemon only
paypunk paypunkd                       # launch app daemon only
paypunk tui                            # launch TUI only (daemons must be running)
paypunk generate-seed -p <password>    # CLI: generate a new wallet
paypunk get-balance --protocol zcash   # CLI: check balance
```

### Running the TUI

The simplest way — auto-launches both daemons and opens the TUI:

```bash
paypunk
```

To run the TUI against already-running daemons (e.g. daemons started separately or on another machine):

```bash
paypunk tui
```

The TUI can also connect to an offline signer instead of a local keypunkd:

```bash
paypunk tui --signer
```

Keybindings within the TUI:

| Key | Action |
|-----|--------|
| `?` | Help overlay (context-sensitive) |
| `Enter` | Select / confirm |
| `Esc` | Back / cancel |
| `q` | Quit |
| `s` | Send |
| `o` | Receive |
| `a` | Add account |
| `r` | Refresh |
| `c` | Copy to clipboard |

### Networks

Paypunk supports Zcash `regtest`, `testnet`, and `mainnet`. The network is selected via the `--zcash-network` flag or the `PAYPUNK_ZCASH_NETWORK` env var. Each network uses its own data directory and default lightwalletd endpoint:

| Network | Lightwalletd default | Data directory |
|---------|---------------------|-----------------|
| `regtest` | `http://127.0.0.1:9067` (local) | `~/.local/share/paypunk/regtest/` |
| `testnet` | `https://testnet.zec.rocks:443` | `~/.local/share/paypunk/testnet/` |
| `mainnet` | `https://zec.rocks:443` | `~/.local/share/paypunk/mainnet/` |

#### Regtest (local development)

Requires a local `zcashd` + `lightwalletd` running on port 9067. See [`support/zcash/README.md`](support/zcash/README.md) for a Docker-based regtest setup.

```bash
# Start the regtest stack
cd support/zcash && make up

# Run paypunk against regtest (default)
paypunk --zcash-network regtest

# Or via env var
PAYPUNK_ZCASH_NETWORK=regtest paypunk
```

To fund your wallet in regtest, mine blocks and shield coinbase to your wallet's address:

```bash
cd support/zcash
make fund UA=<your-orchard-ua>
```

#### Mainnet

Connects to a public lightwalletd endpoint by default. Use a custom endpoint for better privacy or reliability:

```bash
# Using the default public endpoint (https://zec.rocks:443)
paypunk --zcash-network mainnet

# Using a custom lightwalletd
paypunk --zcash-network mainnet --lightwalletd-host https://my-lwd.example.com:443

# Or via env vars
PAYPUNK_ZCASH_NETWORK=mainnet PAYPUNK_LIGHTWALLETD_HOST=https://my-lwd.example.com:443 paypunk
```

#### Ethereum

Ethereum uses an RPC URL (JSON-RPC over HTTP). The default points to a local node (`http://127.0.0.1:8545`); override it for mainnet or testnet:

```bash
# Local anvil/hardhat node (see support/ethereum/README.md)
paypunk --ethereum-rpc-url http://127.0.0.1:8545

# Mainnet (via your own node or provider)
PAYPUNK_ETHEREUM_RPC_URL=https://mainnet.infura.io/v3/<key> paypunk

# Sepolia testnet
PAYPUNK_ETHEREUM_RPC_URL=https://sepolia.infura.io/v3/<key> paypunk
```

#### Configuration file

All defaults can be overridden in `~/.config/paypunk/config.toml`. Generate a template with:

```bash
paypunk  # creates the config file on first run if it doesn't exist
```

See [`config/src/lib.rs`](config/src/lib.rs) for all available fields and env var overrides.
