# PRD

## Problem Statement

Businesses and individuals want to accept and pay with Zcash (private cryptocurrency) but the current focus is on mobile applications and terminal based tooling is mainly linked to running a full node, requires expertise in blockchain infrastructure, and offers poor integration for desktop applications and agentic workflows. There needs to be a simple, secure, non-custodial wallet that works for both human users and autonomous agents, enabling privacy-preserving commerce that is a delight to use without requiring deep Zcash protocol knowledge.

## Solution

Build Paypunk Wallet — a shielded Zcash wallet tool with layered interfaces:

1. **Wallet API** — Core Rust library providing Zcash operations (address generation, balance tracking, transaction construction) via blind LSP chain scanning
2. **CLI** — Command-line interface wrapping the wallet API for scripting and automation
3. **TUI** — Terminal-based user interface replacing CLI for interactive human use

The architecture is designed to eventually support a Tauri desktop interface, agent-to-agent commerce flows, and FROST multi-signature workflows where an agent proposes transactions that require human approval.

## User Stories

1. As a privacy-conscious wallet user, I want to generate a new wallet with a BIP39 seed phrase, so that I can back up my keys offline
2. As a privacy-conscious wallet user, I want to restore my wallet from a saved seed phrase and password, so that I can recover my funds on a different device
3. As a privacy-conscious wallet user, I want the ability to generate a new Address for each Incoming Payment, so that my payment history cannot be linked by observers
4. As a privacy-conscious wallet user, I want to check my available ZEC balance at any time, so that I know how much I can spend
5. As a privacy-conscious wallet user, I want to initiate a Transfer to another Zcash address with an amount and optional memo, so that I can pay for goods and services privately
6. As a privacy-conscious wallet user, I want to view my transaction history with confirmation status (pending/confirmed), so that I can track my payments
7. As a privacy-conscious wallet user, I want my seed to be encrypted at rest with Argon2 derived from my password, so that my keys are protected against unauthorized access
8. As a privacy-conscious wallet user, I want my wallet state database to be encrypted separately from my seed encryption, so that there is security compartmentalization
9. As a privacy-conscious wallet user, I want the wallet to scan for Incoming Payments without exposing my view keys to external servers, so that my financial data remains private
10. As a privacy-conscious wallet user, I want the wallet to connect to public LSP endpoints by default, so that I can start using it without running infrastructure
11. As a CLI user, I want to unlock my wallet with a password prompt or environment variable, so that I can use the wallet interactively or non-interactively
12. As a CLI user, I want to create a new wallet from the command line, so that I can script wallet provisioning
13. As a CLI user, I want to generate addresses from the command line, so that I can get payment destinations programmatically
14. As a CLI user, I want to check balance from the command line, so that I can monitor funds via scripts
15. As a CLI user, I want to send Transfers from the command line, so that I can automate payments
16. As a CLI user, I want to view transaction history from the command line, so that I can audit my wallet activity programmatically
17. As an agent operator, I want to provide my wallet password via a mounted secrets file with restricted permissions, so that my agent can sign Transfers without interactive prompts
18. As an agent, I want to call the wallet API over IPC, so that I can integrate Zcash payments into my workflows
19. As a future TUI user, I want to interactively view my balance and addresses in a terminal interface, so that I can manage my wallet without a web browser
20. As a developer, I want the wallet API to be a separate library crate from my CLI, so that it can be consumed by third-party integrations

## Implementation Decisions

- **Modules tested** — Key derivation, Encryption, Address derivation, Balance tracking, Transfer construction (approved by user as sufficient for v1)
- **Sapling shielded pool only** — Target the Zcash sapling shielded pool exclusively for v1. No transparent balance support at launch.
- **Cargo workspace with sub-crates** — `paypunk` root workspace containing `wallet-api`, `key-daemon`, `wallet-daemon`, and `cli` crates
- **Seed storage** — BIP39 mnemonic stored encrypted at rest with Argon2-derived key from user passphrase
- **Wallet state** — SQLite database encrypted with a separate key also derived from the user's passphrase but distinct from the seed encryption key
- **Dual-daemon architecture** — The Key Daemon holds decrypted private keys in-memory and processes only signing operations. The Wallet Daemon handles all non-secret operations (address derivation, LSP sync, balance tracking, Transfer construction). Communication is abstracted behind an IPC trait for future Unix socket migration.
- **Key isolation boundary** — The Key Daemon must never expose raw private keys over any communication channel. It accepts sign/prove requests and returns only results (signatures, protocol proofs).
- **Blind LSP scanning** — Follow Zashi/zRPC conventions. Send only diversifier prefixes to public LSP servers for block hints, then scan candidates locally. View keys never leave the local machine.
- **Public LSP endpoints** — Ship with default public LSP server list for immediate usability. Allow override via configuration.
- **Address derivation** — Hierarchical deterministic (BIP32/44) key derivation from seed, producing a unique shielded receiving address for each new Incoming Payment opportunity.
- **Transaction broadcast** — Via the same LSP/zRPC connection used for chain scanning
- **Passphrase input** — Support both interactive CLI prompt and environment variable for non-interactive/scripted usage

## Testing Decisions

- **Testing philosophy** — Only test external behavior, not internal implementation details. Tests should verify that given certain inputs, the correct outputs are produced across the module boundary.
- **Modules to test**:
  - **Key derivation module** (deep) — Seed generation/validation, BIP39 recovery phrase handling, BIP32/44 path derivation from seed, Argon2 key derivation correctness
  - **Encryption module** (deep) — Encrypt/decrypt roundtrip for both seed and SQLite storage keys with Argon2 derived keys
  - **Address derivation module** (deep) — Deterministic address generation: given a seed + path, always produces the same shielded address. Uniqueness across paths. Diversifier prefix extraction correctness.
  - **Balance tracking module** (deep) — Given a set of known tx outputs and spending proofs, compute correct available balance
  - **Transfer construction module** (deep) — Given inputs (source wallet, destination address, amount, memo), produce a valid shielded transaction that the Key Daemon can sign. Invalid input rejection (insufficient balance, invalid address).

## Out of Scope

- Transparent pool balances and transactions (planned for later phase)
- Invoice generation and payment request processing (planned for later phase)
- Subscription/recurring payments
- FROST multi-signature / agent approval workflows (post-v1)
- n8n integration and merchant invoicing tools (separate product)
- TUI interface (after CLI is functional)
- Tauri desktop interface (future migration target)

## Further Notes

The dual-daemon architecture currently runs in-process with abstracted IPC. The intent is to separate into two independent processes using Unix domain sockets when agent isolation requirements become concrete. This keeps development velocity high for v1 while preserving the security boundary as an architectural invariant.
