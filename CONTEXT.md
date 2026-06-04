# Paypunk

Zcash wallet infrastructure for privacy-preserving commerce on desktop and agentic workflows.

## Language

**Wallet**:
A Zcash key manager capable of generating Addresses, checking Balance, building outgoing Transfers, and scanning the chain for incoming funds via LSP. Split across three processes: KeyActor in keypunkd, WalletActor in paypunkd, IPC routing via the ipc crate.
_Avoid_: Vault, safe

**KeyActor**:
An actor (tactix) that holds the decrypted spending key in protected memory. Lives inside `keypunkd`. The security boundary — only accepts `Unlock`, `Lock`, `SignTransaction`, and `Prove` messages. Never exposes raw key material.
_Avoid_: Key Daemon, signer

**WalletActor**:
An actor (tactix) managing non-secret operations: address derivation, LSP sync, balance tracking, transfer construction. Lives inside `paypunkd`. Owns the SQLite wallet state database. Delegates signing to the KeyActor (in `keypunkd`) via IPC when a transfer needs finalization.
_Avoid_: Wallet Daemon

**Seed**:
A 12-word BIP39 mnemonic phrase from which all wallet keys are deterministically derived. Stored at rest in a dedicated file (`seed.enc`), encrypted with an Argon2id-derived key from the user's password. The seed file is eventually owned by a different system user than the wallet process for security compartmentalization.

**Address**:
A unique receiving address derived for each incoming payment. One address per payment — never reused (post-v1 goal; address reuse is acceptable for initial build).
_Avoid_: Reuse

**Transfer**:
An outbound payment from the wallet to a recipient's Zcash Address, including an Amount and an optional Memo. Initiated by the user or an agent acting on their behalf.
_Avoid_: Transaction (ambiguous with chain-level tx), sending

**Incoming Payment**:
Funds received into the wallet detected via LSP chain scanning of the current Address.
_Avoid_: Receipt

**keypunkd**:
Long-running daemon hosting the KeyActor. Responsible for key generation, signing, and proving. Runs as a separate system user for defense-in-depth (file/memory isolation). IPC auth is per-message HMAC using X25519 shared secret — any process can connect, but only a client holding the registered keypair can send valid messages. Password is additionally required for `Unlock`. See ADR-001.
_Avoid_: Key daemon

**paypunkd**:
Long-running daemon hosting the WalletActor, usecases, and service orchestration. Exposes IPC over Unix socket. Runs as the user's login UID. Never holds key material — delegates signing to keypunkd via IPC.
_Avoid_: App daemon

**ipc**:
Library crate providing a tactix actor that serializes/deserializes messages with postcard over Unix domain sockets. The communication sender between all processes. api, paypunkd, and keypunkd all use it.
_Avoid_: Transport, wire

**api**:
Public-facing library that CLI and TUI depend on. Provides high-level functions (`get_balance`, `create_transfer`, etc.) that accept an asset type and dispatch to the appropriate chain backend. Hides IPC/tactix details from consumers. Internally communicates with paypunkd via the ipc crate.
_Avoid_: SDK

**protocols**:
Directory of chain-specific implementation crates (e.g., `protocols/zcash`, `protocols/ethereum`). Each implements the `ChainService` trait from paypunkd::services.
_Avoid_: adapters

## Architecture

- Single context repo. No CONTEXT-MAP.md needed.
- Three-process architecture: `keypunkd` (key daemon), `paypunkd` (app daemon), and `paypunk` (CLI/TUI)
- Layers: paypunk (CLI/TUI) → api → ipc → paypunkd → ipc → keypunkd
- IPC: Unix domain socket, serde + postcard, tactix actor wrapping each connection

## Product Layers

**api**: Chain-agnostic library providing the public API. Accepts an asset type to dispatch to the correct chain backend. Hides IPC and actor details from consumers.

**keypunkd**: Key daemon — hosts KeyActor. Seed generation, signing, proving. Runs as a separate system user.

**paypunkd**: App daemon — hosts WalletActor, usecases, service orchestration, chain backend injection.

**paypunk**: CLI binary. Connects to paypunkd via api. Includes TUI mode (ratatui) for interactive use.

**TUI** (future Tauri): Terminal-based user interface, ships inside the CLI binary. Planned migration to Tauri later.

## Data Model

All entity types are chain-agnostic primitives (strings, numbers, enums). No generics or trait objects on the data types. Chain-specific logic lives inside protocol implementation crates (`protocols/zcash`, `protocols/ethereum`).

**Types**: Address(String), Amount(u64), TransferId(String), BlockHeight(u64), Balance { spendable, pending, total }, TransactionStatus { Pending, Confirmed(BlockHeight), Failed(String) }, Transfer { id, from, to, amount, fee, memo, status, created_at }
