# Paypunk

Zcash wallet infrastructure for privacy-preserving commerce on desktop and agentic workflows.

## Product Layers

**Wallet API**: Core library providing Zcash operations. The foundation everything else is built on.

**CLI**: Command-line interface wrapping the wallet API. Minimal, scriptable.

**TUI**: Terminal-based user interface. Replaces CLI for interactive use. Planned migration to Tauri later.

## Language

**Wallet**:
A Zcash key manager capable of generating Addresses, checking Balance, building outgoing Transfers, and scanning the chain for incoming funds via LSP. Split into two actors: KeyActor and WalletActor.
_Avoid_: Vault, safe

**KeyActor**:
An actor (tactix) that holds the decrypted spending key in protected memory. The security boundary — only accepts `Unlock`, `Lock`, `SignTransaction`, and `Prove` messages. Never exposes raw key material.
_Avoid_: Key Daemon, signer

**WalletActor**:
An actor (tactix) managing non-secret operations: address derivation, LSP sync, balance tracking, transfer construction. Owns the SQLite wallet state database. Delegates signing to the KeyActor when a transfer needs finalization.
_Avoid_: Wallet Daemon

**Seed**:
A 12-word BIP39 mnemonic phrase from which all wallet keys are deterministically derived. Stored at rest in a dedicated file (`seed.enc`), encrypted with an Argon2id-derived key from the user's password. The seed file is eventually owned by a different system user than the wallet process for security compartmentalization.

**Address**:
A unique receiving address derived for each incoming payment. One address per transaction — never reused.

**Transfer**:
An outbound payment from the wallet to a recipient's Zcash Address, including an Amount and an optional Memo. Initiated by the user or an agent acting on their behalf.
_Avoid_: Transaction (ambiguous with chain-level tx), sending

**Incoming Payment**:
Funds received into the wallet detected via LSP chain scanning of the current Address.
_Avoid_: Receipt

## Architecture

- Single context repo. No CONTEXT-MAP.md needed.
- Layers: Wallet API → CLI → TUI → (future) Tauri desktop interface
