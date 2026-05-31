# keypunkd — Key Daemon Specification

## Overview

Long-running daemon hosting the KeyActor. Responsible for key generation, signing, and proving. Runs as a separate system user with restricted access. Only accepts IPC from `paypunkd`.

## Phase 1: Wallet Generation

The first milestone is the **wallet generation usecase** — keypunkd listens on a Unix socket, accepts a `GenerateSeed` command over IPC, creates a BIP39 seed, encrypts it with an Argon2id-derived key, and persists it to `seed.enc`.

### Message Flow

```
client (test / paypunkd) → ipc (raw bytes) → keypunkd dispatcher → deserialize → key module → serialize → ipc (raw bytes) → client
```

### IPC Protocol (keypunkd-specific)

Serialization/deserialization of application-level messages is handled by each side using `serde` + `postcard`. The `ipc` crate provides the raw-bytes transport; keypunkd defines its own request/response types.

```rust
/// Requests that keypunkd can handle.
#[derive(Serialize, Deserialize)]
enum KeypunkdRequest {
    GenerateSeed {
        password: String,
    },
}

/// Responses from keypunkd.
#[derive(Serialize, Deserialize)]
enum KeypunkdResponse {
    /// Seed generated, encrypted, and written to disk.
    SeedGenerated,
    /// Operation failed with a human-readable error.
    Error { message: String },
}
```

### Key Module

Pure functions for seed generation and encryption.

```rust
struct Seed([u8; 64]); // 512-bit seed from BIP39 mnemonic

fn generate_seed() -> Seed;
fn encrypt_seed(seed: &Seed, password: &str) -> Result<Vec<u8>>; // encrypted blob
fn write_seed_file(encrypted: &[u8], path: &Path) -> Result<()>;
```

- `generate_seed`: Uses `bip39` crate to generate a 12-word mnemonic, derives the 512-bit seed via PBKDF2.
- `encrypt_seed`: Derives an encryption key from `password` using Argon2id, encrypts the seed bytes with AES-256-GCM, returns the encrypted blob (nonce + ciphertext).
- `write_seed_file`: Atomically writes the encrypted blob to `seed.enc` (write to temp file, rename).

### Daemon Entry Point

```
keypunkd [--socket-path /path/to/keypunkd.sock] [--data-dir /path/to/data]
```

1. Parse CLI args
2. Bind Unix socket at `--socket-path` (default: `$DATA_DIR/keypunkd.sock`)
3. Create dispatcher actor that handles `KeypunkdRequest` messages
4. Start `IpcServer` with the dispatcher as handler
5. Loop accepting connections until shutdown

### Dispatcher Actor

A tactix actor that:
1. Receives `IpcMessage` (raw bytes)
2. Deserializes bytes → `KeypunkdRequest` via `postcard`
3. Matches on the request variant:
   - `GenerateSeed { password }` → call key module functions
4. Serializes `KeypunkdResponse` → response bytes
5. Returns bytes through the IPC actor

### Directory Structure

```
keypunkd/
├── SPEC.md              # This file
├── Cargo.toml
└── src/
    ├── main.rs           # CLI entry point, daemon bootstrap
    ├── dispatcher.rs     # Tactix actor: deserialize → dispatch → serialize
    ├── key.rs            # Seed generation, encryption, file I/O
    └── messages.rs       # KeypunkdRequest, KeypunkdResponse types
```

### Dependencies

| Crate | Purpose |
|-------|---------|
| `paypunk-ipc` | Raw-bytes IPC transport (IpcServer, IpcActor, IpcMessage) |
| `tactix` | Actor framework |
| `tokio` | Async runtime |
| `serde` / `postcard` | Message serialization |
| `bip39` | BIP39 mnemonic generation |
| `argon2` | Argon2id key derivation |
| `aes-gcm` | AES-256-GCM encryption |
| `clap` | CLI argument parsing |
| `thiserror` | Error types |

### Validation Checkpoint

A test that:
1. Starts keypunkd on a temp socket path
2. Connects via `IpcActor`
3. Sends a `GenerateSeed` request with a password
4. Receives `SeedGenerated` response
5. Verifies `seed.enc` exists and is non-empty
6. (Optional) Verifies the encrypted blob can be decrypted with the same password

### Build Checklist (Phase 1)

| # | Task | Status |
|---|------|--------|
| 1 | Create `keypunkd` crate with `Cargo.toml` | ☐ Pending |
| 2 | Define `KeypunkdRequest` / `KeypunkdResponse` message types | ☐ Pending |
| 3 | Implement `key` module: `generate_seed`, `encrypt_seed`, `write_seed_file` | ☐ Pending |
| 4 | Implement `dispatcher` actor: deserialize → dispatch → serialize | ☐ Pending |
| 5 | Implement `main.rs`: CLI args, socket bind, server loop | ☐ Pending |
| 6 | End-to-end test: start daemon, send GenerateSeed, verify seed.enc | ☐ Pending |
