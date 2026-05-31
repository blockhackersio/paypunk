# PRD

## Problem Statement

Businesses and individuals want to accept and pay with Zcash (private cryptocurrency) but the current focus is on mobile applications and terminal based tooling is mainly linked to running a full node, requires expertise in blockchain infrastructure, and offers poor integration for desktop applications and agentic workflows. There needs to be a simple, secure, non-custodial wallet that works for both human users and autonomous agents, enabling privacy-preserving commerce that is a delight to use without requiring deep Zcash protocol knowledge.

## Solution

Build Paypunk Wallet — a Zcash wallet tool with layered interfaces, targeting all pools (Sapling, Orchard, and transparent):

1. **Wallet API** — Core Rust library providing Zcash operations (key derivation, address generation, balance tracking, LSP chain scanning, transfer construction) via a tactix actor model
2. **CLI** — Command-line interface wrapping the wallet API for scripting and automation. Integrates the TUI as a library for interactive use.
3. **TUI** — Terminal-based user interface (ratatui) for interactive human use. Ships alongside the CLI as a reusable library crate.

The architecture is designed to eventually support a Tauri desktop interface, agent-to-agent commerce flows, and FROST multi-signature workflows where an agent proposes transactions that require human approval.

### Target User (v1)

Individual privacy-conscious users, including developers and agent operators running their own wallet. Businesses raising and paying invoices is the long-term goal (via a sister project) but not the v1 target.

## User Stories

1. As a privacy-conscious wallet user, I want to generate a new wallet with a 12-word BIP39 seed phrase, so that I can back up my keys offline
2. As a privacy-conscious wallet user, I want to restore my wallet from a saved seed phrase and password, so that I can recover my funds on a different device
3. As a privacy-conscious wallet user, I want to check my available ZEC balance at any time, so that I know how much I can spend
4. As a privacy-conscious wallet user, I want to initiate a Transfer to another Zcash address with an amount and optional memo, so that I can pay for goods and services privately
5. As a privacy-conscious wallet user, I want to view my transaction history with confirmation status (pending/confirmed), so that I can track my payments
6. As a privacy-conscious wallet user, I want my seed to be encrypted at rest with Argon2id derived from my password, so that my keys are protected against unauthorized access
7. As a privacy-conscious wallet user, I want my wallet state database to be encrypted separately from my seed encryption, so that there is security compartmentalization
8. As a privacy-conscious wallet user, I want the wallet to scan for Incoming Payments without exposing my view keys to external servers, so that my financial data remains private
9. As a privacy-conscious wallet user, I want the wallet to connect to public lightwalletd endpoints by default, so that I can start using it without running infrastructure
10. As a CLI user, I want to unlock my wallet with a password prompt, environment variable, or mounted secrets file, so that I can use the wallet interactively or non-interactively
11. As a CLI user, I want to create a new wallet from the command line, so that I can script wallet provisioning
12. As a CLI user, I want to generate addresses from the command line, so that I can get payment destinations programmatically
13. As a CLI user, I want to check balance from the command line, so that I can monitor funds via scripts
14. As a CLI user, I want to send Transfers from the command line, so that I can automate payments
15. As a CLI user, I want to view transaction history from the command line, so that I can audit my wallet activity programmatically
16. As a CLI user, I want to sync the wallet chain state on demand, so that I can see recent activity without running a background process
17. As an agent operator, I want to provide my wallet password via a mounted secrets file with restricted permissions, so that my agent can sign Transfers without interactive prompts
18. As an agent, I want to call the wallet API over IPC, so that I can integrate Zcash payments into my workflows
19. As a TUI user, I want to interactively view my balance, addresses, and transaction history in a terminal interface, so that I can manage my wallet without a web browser
20. As a TUI user, I want the wallet to stay synced in the background while I use the interface, so that I always see up-to-date information
21. As a developer, I want the wallet API to be a separate library crate from my CLI and TUI, so that it can be consumed by third-party integrations

## Implementation Decisions

### Architecture

- **Actor model** — Two tactix actors with typed message protocols, running in-process for v1. Future process separation uses Unix domain sockets with the same message types.
- **KeyActor** — Holds the decrypted spending key in process-protected memory (mlock). Security boundary — only accepts `Unlock`, `Lock`, `SignTransaction`, and `Prove` messages. Never exposes raw key material.
- **WalletActor** — Manages non-secret operations: address derivation, LSP sync via `zcash_client_backend`, balance tracking, transfer construction. Owns the SQLite wallet state database. Delegates signing to the KeyActor.
- **Key isolation** — The KeyActor must never expose raw private keys. It accepts sign/prove requests and returns only results (signatures, protocol proofs).

### Crate Layout

- **`wallet-api`** (library) — Defines actors, message types, and all wallet logic (key derivation, encryption, LSP scanning, address derivation, transfer construction, balance tracking). Uses `zcash_client_backend`, `zcash_client_sqlite`, `zcash_keys`, `zcash_primitives`, `zcash_protocol`, `zcash_proofs`.
- **`tui`** (library) — Ratatui screens and widgets. Reusable by future Tauri desktop app.
- **`cli`** (binary) — Links both `wallet-api` and `tui`. Runs in CLI mode (single command) or TUI mode (interactive session).

Future crates: `key-daemon` (binary running KeyActor over socket), `wallet-daemon` (binary running WalletActor over socket) — created when process-split is needed.

### Pool Support

- All pools in v1: Sapling (shielded), Orchard (shielded), and transparent (non-shielded).
- Default to Orchard for outgoing transactions where possible.
- Transparent support via `transparent-inputs` feature on `zcash_client_backend`.

### Seed Lifecycle

- **Creation**: 12-word BIP39 mnemonic. User confirms by re-entering words.
- **Storage**: Encrypted seed in a dedicated file (`seed.enc`) separate from the wallet state SQLite DB. This separation allows wiping and resyncing state without losing the seed. Eventually the seed file is owned by a different system user than the wallet process.
- **Restore**: User enters 12-word mnemonic + sets a new password. The encrypted seed file is written from scratch.
- **Unlock**: KeyActor reads the encrypted seed file, decrypts with Argon2id-derived key, derives `UnifiedSpendingKey`, holds it in protected memory for the session.

### Encryption

- Single Argon2id call from user password → one master key → HKDF-split into two sub-keys: one for seed file encryption, one for SQLite wallet state encryption.
- This avoids paying Argon2id cost twice while keeping the two encryption domains independent.

### LSP Scanning

- **v1 trigger**: Manual `paypunk sync` command + automatic sync on startup. WalletActor resumes from the last-scanned height stored in SQLite.
- **Endpoints**: Ship with 2-3 public lightwalletd endpoints, round-robin with fallback on failure. Allow override via configuration.
- **TUI**: Background polling loop added when the TUI ships (long-running process).
- **Privacy**: Blind LSP scanning — send only diversifier prefixes for block hints, scan candidates locally. View keys never leave the machine.

### Address Derivation

- Hierarchical deterministic (BIP32/44 / ZIP 32) key derivation from seed.
- Produces a unique shielded receiving address for each new Incoming Payment opportunity.
- One address per transaction — never reused.

### Transaction Broadcast

- Via the same lightwalletd gRPC connection used for chain scanning.

### Passphrase Input

- Support interactive CLI prompt, environment variable, and mounted secrets file for non-interactive/scripted/agent usage.

### Configuration

- Data directory (default `~/.paypunk/`)
- LSP endpoint list (with defaults)
- Secrets file path (for agent mode)

## Testing Decisions

- **Testing philosophy** — Only test external behavior, not internal implementation details. Tests should verify that given certain inputs, the correct outputs are produced across the module boundary.
- **Modules to test**:
  - **Key derivation module** (deep) — Seed generation/validation, BIP39 recovery phrase handling, BIP32/44/ZIP 32 path derivation from seed, Argon2id key derivation correctness
  - **Encryption module** (deep) — Encrypt/decrypt roundtrip for both seed and SQLite storage keys with Argon2id-derived keys, HKDF splitting correctness
  - **Address derivation module** (deep) — Deterministic address generation: given a seed + path, always produces the same address. Uniqueness across paths. Diversifier prefix extraction correctness.
  - **Balance tracking module** (deep) — Given a set of known tx outputs and spending proofs, compute correct available balance across all pools
  - **Transfer construction module** (deep) — Given inputs (source wallet, destination address, amount, memo), produce a valid transaction proposal that the KeyActor can sign. Invalid input rejection (insufficient balance, invalid address, unsupported pool).

## Out of Scope

- Invoice generation and payment request processing (planned for sister project)
- Subscription/recurring payments
- FROST multi-signature / agent approval workflows (post-v1)
- n8n integration and merchant invoicing tools (separate product)
- Tauri desktop interface (future migration target)
- OS keyring integration (post-v1 enhancement)
- Separate daemon processes (added when agent isolation requirements are concrete)

## Further Notes

The actor model currently runs both KeyActor and WalletActor in-process using tactix mailboxes. The message types between them are the IPC contract — when process separation is needed, each actor gets extracted into its own binary with Unix socket transport, but the message protocol doesn't change. This keeps development velocity high for v1 while preserving the security boundary as an architectural invariant.
