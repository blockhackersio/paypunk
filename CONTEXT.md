# Paypunk

Zcash wallet infrastructure for privacy-preserving commerce on desktop and agentic workflows.

## Product Layers

**Wallet API**: Core library providing shielded Zcash operations. The foundation everything else is built on.

**CLI**: Command-line interface wrapping the wallet API. Minimal, scriptable.

**TUI**: Terminal-based user interface. Replaces CLI for interactive use. Planned migration to Tauri later.

## Language

**Wallet**:
A shielded Zcash key manager capable of generating Addresses, checking Balance, building outgoing Transfers, and scanning the chain for incoming funds via LSP. Split into two components: Key Daemon and Wallet Daemon.
_Avoid_: Vault, safe

**Seed**:
A BIP39 mnemonic phrase from which all wallet keys are deterministically derived. Stored at rest encrypted with a user-supplied password. Agent use holds password in a mounted secrets file.

**Key Daemon**:
A process that holds the decrypted private key in memory and handles signing operations (signatures, proofs). Never exposes raw keys — only accepts sign/prove requests and returns results. Derived from wallet API crate.

**Wallet Daemon**:
A process managing non-secret operations: Address derivation, LSP sync, balance tracking, Transfer construction. Delegates signing to the Key Daemon when needed. Derived from wallet API crate. Scans chain via blind LSP — never shares view keys with external servers, only sends diversifier prefixes and scans locally. Follows Zashi/zRPC protocol conventions.

**Address**:
A unique shielded receiving address derived for each incoming payment. One address per transaction — never reused.

**Transfer**:
An outbound payment from the wallet to a recipient's Zcash Address, including an Amount and an optional Memo. Initiated by the user or an agent acting on their behalf.
_Avoid_: Transaction (ambiguous with chain-level tx), sending

**Incoming Payment**:
Funds received into the wallet detected via LSP chain scanning of the current Address.
_Avoid_: Receipt

## Architecture

- Single context repo. No CONTEXT-MAP.md needed.
- Layers: Wallet API → CLI → TUI → (future) Tauri desktop interface
