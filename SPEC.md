# Paypunk — Technical Specification

## 1. Architecture

### 1.1 Shape

Two-process architecture from v1:

- **`paypunkd`** — Long-running daemon hosting KeyActor + WalletActor. Exposes IPC over Unix domain socket.
- **`paypunk`** — CLI binary. Connects to daemon over Unix socket for all operations. Includes TUI mode (ratatui) for interactive use.

Rationale: Process separation from day one enforces the security boundary — the CLI never holds key material. The daemon can run as a different system user. The message protocol between actors doubles as the IPC contract, so future changes (e.g., network transport) don't require protocol changes.

### 1.2 Stack

| Layer | Choice |
|-------|--------|
| Runtime | Rust (stable), Tokio async runtime |
| Actor framework | tactix |
| IPC | Unix domain socket (serde + bincode) |
| Database | SQLite via `zcash_client_sqlite` |
| gRPC client | lightwalletd via `zcash_client_backend` |
| TUI | ratatui |
| CLI args | clap |
| Encryption | Argon2id + HKDF |
| Key derivation | BIP39 (12-word mnemonic), BIP32/44 / ZIP 32 |

### 1.3 Repository Structure

```
paypunk/
├── wallet-api/           # Core library: actors, messages, wallet logic
│   └── src/
│       ├── actors/       # KeyActor, WalletActor
│       ├── key/          # Seed generation, derivation, encryption
│       ├── scanning/     # LSP chain scanning
│       ├── transfer/     # Transfer construction
│       └── balance/      # Balance tracking
├── paypunkd/             # Daemon binary (hosts actors, Unix socket listener)
│   └── src/
│       ├── server/       # Unix socket listener, request dispatch
│       └── main.rs
├── tui/                  # Ratatui screens and widgets (library crate)
│   └── src/
│       ├── screens/      # Full-screen views (dashboard, send, history)
│       └── widgets/      # Reusable UI components
├── cli/                  # CLI binary (connects to daemon, includes TUI mode)
│   └── src/
│       ├── commands/     # Subcommand implementations (IPC client calls)
│       ├── config/       # Config loading, socket path, endpoints
│       └── main.rs
└── tests/                # Integration tests
```

### 1.4 Architecture Decision Records

(To be created as decisions are made.)

## 2. Data Model

### 2.1 Design Approach

Uniform chain-agnostic types. Data flowing through actors and IPC uses common-denominator primitives (strings, numbers, enums) rather than generic traits or type parameters. Chain-specific logic is encapsulated inside the actor implementations; the data model itself is blockchain-agnostic.

```rust
struct Address(String);              // "u1..." for Zcash, "0x..." for Ethereum
struct Amount(u64);                  // zatoshis / wei / satoshis
struct TransferId(String);           // tx hash hex
struct BlockHeight(u64);             // block number

struct Balance {
    spendable: Amount,
    pending: Amount,
    total: Amount,
}

enum TransactionStatus {
    Pending,
    Confirmed(BlockHeight),
    Failed(String),
}

struct Transfer {
    id: TransferId,
    from: Address,
    to: Address,
    amount: Amount,
    fee: Amount,
    memo: Option<String>,
    status: TransactionStatus,
    created_at: OffsetDateTime,
}
```

### 2.2 Domain Entities

| Entity | Key fields | Storage | Notes |
|--------|-----------|---------|-------|
| `Seed` | mnemonic (12 words), encrypted_blob, created_at | `seed.enc` file | Not in SQLite |
| `AccountBirthday` | birthday_height, sapling_frontier, orchard_frontier, recover_until | SQLite (`zcash_client_sqlite`) | Used for LSP scan start |
| `Account` | account_id (ZIP 32 index), ufvk | SQLite (`zcash_client_sqlite`) | Single account in v1 |
| `Address` | index, unified_address, diversifier, pool, account_id | SQLite | One per payment, never reused |
| `Transfer` | raw tx, outputs, fee, status, account_id | SQLite (`zcash_client_sqlite`) | Status: Pending → Confirmed → Failed |
| `IncomingPayment` | tx_id, amount, memo, block_height, pool | SQLite (`zcash_client_sqlite`) | Detected via LSP scan |
| `ScanState` | last_scanned_height, fully_scanned_height | SQLite (`zcash_client_sqlite`) | Managed by backend |

### 2.3 Database Schema

Managed by `zcash_client_sqlite`. Our code does not define the schema — it is created and migrated by the upstream crate. We interact via the `WalletRead`/`WalletWrite` traits.

### 2.4 State Machines

**Transfer**

- States: `Pending`, `Confirmed`, `Failed`
- Transitions:
  - `Pending` → `Confirmed`: guard = `mined in block`, side effect = `update balance`
  - `Pending` → `Failed`: guard = `chain rejection / timeout`, side effect = `release reserved funds`
- Invariants: INV-01: "a Transfer amount must never exceed the spendable balance at construction time"

### 2.5 Domain Invariants
- **INV-01**: A Transfer amount + fee must never exceed the spendable balance at construction time.
- **INV-02**: Addresses must never be reused for different Incoming Payments.
- **INV-03**: The KeyActor must never expose raw key material — only signed/proved outputs.

## 3. Module Specification

### `wallet-api` crate

#### `types`
- **Responsibility**: Uniform chain-agnostic data types
- **Dependencies**: none
- **Key interfaces**:
  ```rust
  struct Address(String);
  struct Amount(u64);
  struct TransferId(String);
  struct BlockHeight(u64);
  struct Balance { spendable: Amount, pending: Amount, total: Amount }
  enum TransactionStatus { Pending, Confirmed(BlockHeight), Failed(String) }
  struct Transfer { id: TransferId, from: Address, to: Address, amount: Amount, fee: Amount, memo: Option<String>, status: TransactionStatus, created_at: OffsetDateTime }
  ```

#### `key`
- **Responsibility**: Seed generation, BIP39 mnemonic, Argon2id encryption/decryption, HKDF key splitting
- **Dependencies**: `types`
- **Key interfaces**:
  ```rust
  fn generate_seed() -> Seed;
  fn encrypt_seed(seed: &Seed, password: &str) -> EncryptedSeed;
  fn decrypt_seed(encrypted: &EncryptedSeed, password: &str) -> Result<Seed>;
  fn derive_key(password: &str) -> (StorageKey, SeedKey); // HKDF split
  ```

#### `actors`
- **Responsibility**: KeyActor and WalletActor definitions, message types, actor protocol
- **Dependencies**: `types`, `key`
- **Key interfaces**:
  ```rust
  enum KeyActorMessage {
      Unlock { password: String },
      Lock,
      SignTransaction { proposal: Vec<u8> },
      Prove { proposal: Vec<u8> },
  }
  enum WalletActorMessage {
      GetBalance { resp: ReplyTo<Balance> },
      GetAddress { resp: ReplyTo<Address> },
      CreateTransfer { to: Address, amount: Amount, memo: Option<String>, resp: ReplyTo<Transfer> },
      Sync { resp: ReplyTo<()> },
      GetHistory { resp: ReplyTo<Vec<Transfer>> },
  }
  ```

#### `zcash`
- **Responsibility**: Zcash-specific logic — address derivation via ZIP 32, LSP chain scanning via `zcash_client_backend`, transfer construction, balance computation
- **Dependencies**: `types`, `actors`
- **Critical logic**: Wraps `zcash_client_sqlite` traits (`WalletRead`/`WalletWrite`/`InputSource`), manages lightwalletd gRPC connection, orchestrates scan → decrypt → witness → build → sign → broadcast pipeline

#### `ipc`
- **Responsibility**: Unix socket protocol — request/response serialization, client connection stub, server connection handler
- **Dependencies**: `types`, `actors`
- **Key interfaces**:
  ```rust
  enum IpcRequest {
      GetBalance,
      GetAddress,
      CreateTransfer { to: String, amount: u64, memo: Option<String> },
      Sync,
      GetHistory,
      Unlock { password: String },
      Lock,
  }
  enum IpcResponse {
      Balance(Balance),
      Address(String),
      Transfer(Transfer),
      SyncComplete,
      History(Vec<Transfer>),
      Ok,
      Error(String),
  }
  ```

### `paypunkd` crate
- **Responsibility**: Long-running daemon. Initializes SQLite, spawns KeyActor + WalletActor, listens on Unix socket, dispatches IPC requests to actors
- **Dependencies**: `wallet-api`
- **Startup sequence**: Load config → init SQLite → spawn actors → bind Unix socket → accept loop

### `cli` crate
- **Responsibility**: CLI binary. Connects to daemon Unix socket, sends IPC requests, formats output. Includes TUI mode.
- **Dependencies**: `wallet-api`, `tui`
- **Subcommands**: `init`, `balance`, `address`, `send`, `history`, `sync`, `tui`

### `tui` crate
- **Responsibility**: Ratatui screens and widgets for interactive wallet management
- **Dependencies**: `wallet-api`
- **Screens**: Dashboard (balance + recent transfers), Send form, History list, Sync status

## 4. Critical Logic

### 4.1 Concurrency Model

- **KeyActor**: Sequential message processing (tactix mailbox). Single point for signing — serializes all `SignTransaction` and `Prove` requests. Never exposes raw key material.
- **WalletActor**: Sequential message processing. Serializes SQLite access (handled by `zcash_client_sqlite` writer lock). Orchestrates scanning, balance tracking, transfer construction.
- **IPC server**: Accepts connections in a loop, spawns a lightweight async handler per connection. Each handler sends actor messages and awaits responses.
- **No shared mutable state** between actors — communication is message-passing only. No locks needed beyond SQLite's internal write lock.

### 4.2 Scan Pipeline (WalletActor)

1. Connect to lightwalletd gRPC endpoint (round-robin with fallback across configured endpoints)
2. Fetch chain tip height from lightwalletd
3. Determine unscanned block range from `ScanState` (persisted in SQLite)
4. Download compact blocks for the unscanned range
5. Trial-decrypt each block with the account's `UnifiedFullViewingKey`
6. Update note commitment trees (Sapling + Orchard frontiers)
7. Detect and handle reorgs (truncate to last valid height)
8. Update `WalletSummary` with new per-account balances
9. Persist updated `ScanState`

### 4.3 Key Lifecycle (KeyActor)

1. `Unlock` → read `seed.enc` → Argon2id derive decryption key → decrypt seed → derive `UnifiedSpendingKey` via ZIP 32 → hold in protected memory (mlock, mprotect)
2. `SignTransaction` → sign with USK → return signature bytes
3. `Prove` → generate zk-SNARK proof → return proof bytes
4. `Lock` → zero memory (memset + mlock advisory) → drop USK

### 4.4 IPC Request Flow

```
CLI → Unix socket → IpcRequest → daemon dispatcher → WalletActor message
                                                      → (if sign needed) KeyActor message
                                                      → response → CLI
```

## 5. API Contracts

### 5.1 Internal Module Interfaces

Covered in Section 3 (Module Specification) above. Key interfaces are the actor message types (`KeyActorMessage`, `WalletActorMessage`) and the IPC request/response types (`IpcRequest`, `IpcResponse`).

### 5.2 External API Endpoints

None. All interaction is via Unix domain socket IPC. The CLI is the user-facing interface.

## 6. Build Sequence

### Step 1: Core types + key module
- **What to implement**: `wallet-api` crate scaffold, `types` module (Address, Amount, Balance, Transfer, TransactionStatus), `key` module (seed generation, BIP39 mnemonic, Argon2id encrypt/decrypt, HKDF split)
- **Validation checkpoint**: `cargo test` passes, seed encrypt/decrypt roundtrip works
- **Dependencies**: none

### Step 2: Actors + IPC protocol
- **What to implement**: `actors` module (KeyActorMessage, WalletActorMessage enums, ReplyTo pattern), `ipc` module (IpcRequest, IpcResponse, Unix socket client connection stub, server connection handler)
- **Validation checkpoint**: can connect to daemon, send a message, get a response
- **Dependencies**: Step 1

### Step 3: paypunkd daemon
- **What to implement**: `paypunkd` crate, Unix socket listener, actor spawning and wiring, request dispatch loop, config loading (data dir, socket path, LSP endpoints)
- **Validation checkpoint**: daemon starts, accepts connections, responds to IPC requests
- **Dependencies**: Step 2

### Step 4: Zcash integration
- **What to implement**: `zcash` module wrapping `zcash_client_backend`/`zcash_client_sqlite`, address derivation via ZIP 32, LSP chain scanning, transfer construction, balance computation, WalletActor wired to real Zcash operations
- **Validation checkpoint**: can sync with Zcash testnet, get balance, create a transfer
- **Dependencies**: Step 1, Step 2

### Step 5: CLI commands
- **What to implement**: `cli` crate with clap subcommands: `init`, `balance`, `address`, `send`, `history`, `sync`, `tui`; password input modes (interactive prompt, env var, secrets file)
- **Validation checkpoint**: each command works end-to-end against a running daemon
- **Dependencies**: Step 3, Step 4

### Step 6: TUI
- **What to implement**: `tui` crate with ratatui screens (Dashboard with balance + recent transfers, Send form, History list, Sync status indicator); background polling loop for wallet updates
- **Validation checkpoint**: interactive wallet management works in terminal
- **Dependencies**: Step 5

### Step 7: Polish
- **What to implement**: Error handling refinement, structured logging (tracing), config file, documentation, integration tests
- **Validation checkpoint**: manual QA pass across all commands
- **Dependencies**: Step 6

### Deferred (post-v1)
- Tauri desktop app
- Multi-account support
- FROST multi-signature / agent approval workflows
- OS keyring integration
- Agent-to-agent commerce flows
- n8n integration and merchant invoicing tools (sister project)

## 7. Testing Strategy

- **Unit tests**: `key` module (seed gen/validation, encrypt/decrypt roundtrip, HKDF correctness), `zcash` module (address derivation determinism, balance computation)
- **Integration tests**: IPC protocol (connect daemon, send/receive messages), Zcash sync (testnet scanning)
- **E2E tests**: Full CLI command flows against a running daemon on testnet
- **Coverage targets**: 80%+ on `key` module, 70%+ on `zcash` module, smoke tests on CLI + daemon

## 8. Error Handling

- **Hierarchy**: Top-level `Error` enum in `wallet-api` with module-specific variants (KeyError, WalletError, IpcError, ZcashError)
- **Propagation**: Actors return `Result<T, Error>` through ReplyTo channels. IPC layer serializes errors as `IpcResponse::Error(String)`.
- **UI handling**: CLI formats errors as stderr messages. TUI shows error dialogs.

## 9. Security

- **Auth model**: No auth on Unix socket (filesystem permissions protect the socket). Password required to unlock the KeyActor.
- **Secrets management**: Seed encrypted with Argon2id in `seed.enc`. KeyActor holds decrypted key in mlocked memory. Password sourced from stdin, env var, or secrets file.
- **Data protection**: SQLite wallet state encrypted with separate HKDF-derived key. Socket file permissions restricted to owner.
- **Rate limiting**: Not applicable for v1 (local Unix socket, single user).

## 10. Observability

- **Logging**: Structured logging via `tracing`. Info-level for operations (sync start/complete, transfer created), debug-level for scan details, warn/error for failures.
- **Metrics**: Deferred to post-v1.
- **Alerts**: Not applicable for v1.

## 11. CI/CD

- **Pipeline**: lint (clippy) → typecheck → test (unit + integration) → build (release)
- **Environments**: dev (local cargo), CI (GitHub Actions), release (crates.io + GitHub Releases)
- **Migrations**: SQLite schema managed by `zcash_client_sqlite` migrations — no custom migration tooling needed.

## 12. Open Questions

- Which Zcash network for default? Mainnet or testnet?
- How to handle proving parameters? Download on first use or bundle?
- Single account or configurable account count for v1?
- Exact lightwalletd endpoints to ship as defaults?
