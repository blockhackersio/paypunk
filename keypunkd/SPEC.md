# keypunkd — Key Daemon Specification

## Overview

Long-running daemon hosting the KeyActor. Responsible for key generation, signing, and proving. Runs as a separate system user with restricted access. Only accepts IPC from `paypunkd`.

## Phase 1: Wallet Generation

The first milestone is the **wallet generation usecase** — keypunkd listens on a Unix socket, accepts a `GenerateSeed` command over IPC, creates a BIP39 seed, encrypts it with an Argon2id-derived key, and persists it to `seed.enc`. The password and returned mnemonic are encrypted via X25519 + AES-256-GCM so that the plaintext password never crosses the wire.

### Message Flow

```
client (test / paypunkd) → ipc (auth handshake + raw bytes) → keypunkd dispatcher → deserialize → crypto + key modules → serialize → ipc (raw bytes) → client
```

### IPC Protocol (keypunkd-specific)

Serialization/deserialization of application-level messages is handled by each side using `serde` + `postcard`. The `ipc` crate provides the raw-bytes transport with built-in X25519 per-message authentication; keypunkd defines its own request/response types on top.

```rust
/// Requests that keypunkd can handle.
#[derive(Serialize, Deserialize)]
enum KeypunkdRequest {
    /// Retrieve keypunkd's X25519 public key (for encrypting the password).
    GetPublicKey,
    /// Generate a new seed. Password is encrypted to keypunkd's public key.
    GenerateSeed {
        encrypted_password: Vec<u8>,      // AES-GCM encrypted to keypunkd's public key
        client_public_key: [u8; 32],       // Client's ephemeral X25519 public key
    },
}

/// Responses from keypunkd.
#[derive(Serialize, Deserialize)]
enum KeypunkdResponse {
    /// Server's X25519 public key.
    PublicKey { key: [u8; 32] },
    /// Seed generated, encrypted, and written to disk. Mnemonic encrypted to client's public key.
    SeedGenerated {
        encrypted_mnemonic: Vec<u8>,      // Mnemonic encrypted to client's public key
    },
    /// Operation failed with a human-readable error.
    Error { message: String },
}
```

### Key Module

Pure functions for seed generation and encryption.

```rust
fn generate_seed() -> ([u8; 64], String);    // (512-bit seed from BIP39, 12-word mnemonic)
fn derive_key(password: &str, salt: &[u8]) -> [u8; 32];  // Argon2id key derivation
fn encrypt_seed(seed: &[u8; 64], password: &str) -> Result<Vec<u8>>;
```

- `generate_seed`: Uses `bip39` crate to generate a 12-word mnemonic, derives the 512-bit seed via PBKDF2. Returns the seed bytes and mnemonic phrase as a tuple.
- `derive_key`: Derives a 32-byte encryption key from `password` using Argon2id with a provided salt.
- `encrypt_seed`: Calls `derive_key` with a random 16-byte salt, encrypts the 64-byte seed with AES-256-GCM (random 12-byte nonce), returns the encrypted blob formatted as `[16-byte salt][12-byte nonce][AES-256-GCM ciphertext]`.

### Crypto Module

X25519 keypair for encrypted IPC exchange — the password and returned mnemonic never cross the wire in plaintext. Unlike the IPC transport HMAC (which authenticates but does not encrypt), this module provides **confidentiality** for the end-to-end secret exchange between the CLI client and keypunkd.

```rust
/// Keypair used by both server (keypunkd) and client (api).
/// Encrypts/decrypts using X25519 shared secret + AES-256-GCM.
struct Keypair { /* X25519 secret + public */ }
impl Keypair {
    fn new() -> Self;
    fn public_key(&self) -> [u8; 32];
    fn keypair(&self) -> ([u8; 32], [u8; 32]);     // (secret, public)
    fn encrypt<T: Zeroize + AsRef<[u8]>>(&self, secret_message: Zeroizing<T>, peer_pk: &[u8; 32]) -> Vec<u8>;
    fn decrypt(&self, encrypted: &[u8], peer_pk: &[u8; 32]) -> Result<Zeroizing<String>, CryptoError>;
}
```

- Shared secret derived via X25519 DH between the two keypairs.
- AES-256-GCM key derived from shared secret via Blake2b.
- `encrypt`: Encrypts a secret message (password or mnemonic) to a peer's public key. Returns `nonce(12) + ciphertext`.
- `decrypt`: Decrypts a message from a peer. Returns the plaintext as `Zeroizing<String>`.
- Both sides use the same `Keypair` type — there is no separate `KeyStore`/`CryptoSession` split. The server creates one at startup; the client creates one per request.

### Seed Store

Abstraction over persisting the encrypted seed blob.

```rust
trait SeedStore {
    fn write(&self, blob: &[u8]) -> Result<(), SeedStoreError>;
}

/// Writes to seed.enc atomically (write to .enc.tmp, rename).
struct FilesystemSeedStore { path: PathBuf }

/// Holds blob in memory for testing.
struct InMemorySeedStore { blob: Mutex<Option<Vec<u8>>> }
```

### Daemon Entry Point

```
keypunkd [--socket-path /tmp/keypunkd.sock] [--data-dir /tmp/paypunk/data]
```

1. Parse CLI args (`--socket-path`, `--data-dir`)
2. Create `Keypair` (generates X25519 keypair for IPC encryption)
3. Create `FilesystemSeedStore` pointing at `{data_dir}/seed.enc`
4. Create `Dispatcher` actor with keystore and seed store
5. Bind `UnixListener` at `--socket-path` (default: `/tmp/keypunkd.sock`)
6. Create `IpcReceiver::new(listener, secret, public)` — shares the Keypair so the IPC handshake key matches the encryption key
7. Initialize `tracing` subscriber with env-filter support
8. Serve connections until Ctrl+C

### Dispatcher Actor

A tactix actor generic over any `SeedStore` implementation:

```rust
struct Dispatcher<S: Storage> {
    keystore: Keypair,
    seed_store: S,
    session: Option<[u8; 32]>,       // sender_public_key of the active session
    skip_session_auth: bool,          // bypass sender check (for tests)
}
```

Message authentication:
- `verify_message`: Rejects messages where `sender_public_key` is `None` (in-process calls) unless `skip_session_auth` is enabled. This ensures all IPC requests come from an authenticated peer (the IPC handshake sets `sender_public_key`).
- `set_session`: On successful password-authenticated requests (`GenerateSeed`), records the sender's public key as the active session. (Future `Unlock` will do the same.)

Session management:
- `session` stores the `sender_public_key` from the last successful password-authenticated request (`GenerateSeed`, and future `Unlock`).
- `GetPublicKey` is **always allowed** — no session check.
- Requests with `encrypted_password` (e.g., `GenerateSeed`, future `Unlock`) **attempt execution unconditionally**. On success, `session` is set to the request's `sender_public_key`. On failure, an error is returned and `session` is unchanged.
- All other requests **check the current session**: if `msg.sender_public_key` does not match `session`, an error is returned. If `session` is `None`, an error is returned (no active session).

This ensures only one process at a time can hold an authenticated session with keypunkd, especially while the keystore is unlocked.

Message flow:

1. Receives `IpcMessage` (raw bytes with `sender_public_key` set by IPC handshake)
2. `verify_message` — rejects if `sender_public_key` is `None` (in-process message without auth bypass)
3. Deserializes bytes → `KeypunkdRequest` via `postcard`
4. Matches on the request variant:
   - `GetPublicKey` → returns `KeypunkdResponse::PublicKey { key }` (no session check)
   - `GenerateSeed { encrypted_password, client_public_key }` → calls `generate_seed()` usecase:
     - Decrypts password using `Keypair::decrypt()`
     - Calls `key::generate_seed()` to create 64-byte seed + mnemonic
     - Calls `key::encrypt_seed()` with the recovered password
     - Persists via `SeedStore::write()`
     - Encrypts mnemonic back to client's public key via `Keypair::encrypt()`
     - On success, sets `session = msg.sender_public_key`
     - Returns `KeypunkdResponse::SeedGenerated { encrypted_mnemonic }`
5. Serializes `KeypunkdResponse` → response bytes via `postcard`
6. Returns bytes through the IPC actor

### Directory Structure

```
keypunkd/
├── SPEC.md              # This file
├── Cargo.toml
├── tests/
│   └── generate_seed_test.rs  # Integration tests (4 tests)
└── src/
    ├── lib.rs            # Crate root, re-exports public modules
    ├── main.rs           # CLI entry point, daemon bootstrap
    ├── messages.rs       # KeypunkdRequest, KeypunkdResponse types
    ├── dispatcher.rs     # Tactix actor: deserialize → dispatch → serialize, session mgmt
    ├── key.rs            # Seed generation, BIP39, Argon2id encrypt/decrypt
    ├── crypto.rs         # X25519 Keypair for encrypted IPC exchange
    ├── services.rs       # KeypunkService wrapping Recipient<IpcMessage>
    ├── usecases.rs       # Business logic: generate_seed
    ├── errors.rs         # GenerateError enum
    └── seed_store.rs     # SeedStore trait, FilesystemSeedStore, InMemorySeedStore
```

### Dependencies

| Crate | Purpose |
|-------|---------|
| `paypunk-ipc` | Raw-bytes IPC transport with X25519 auth (IpcReceiver, IpcSender, IpcMessage) |
| `tactix` | Actor framework |
| `tokio` | Async runtime |
| `serde` / `postcard` | Message serialization |
| `bip39` | BIP39 mnemonic generation |
| `argon2` | Argon2id key derivation |
| `aes-gcm` | AES-256-GCM encryption |
| `x25519-dalek` | X25519 key exchange for encrypted IPC |
| `blake2` | HMAC key derivation |
| `rand` | Random nonce / salt generation |
| `clap` | CLI argument parsing |
| `thiserror` | Error types |
| `zeroize` | Secure memory clearing |
| `tracing` / `tracing-subscriber` | Structured logging |

### Validation Checkpoint

Tests that:
1. Start keypunkd dispatcher in-process with `InMemorySeedStore`
2. Connect via direct actor `ask()` (simulating IPC with `sender_public_key` set)
3. Send a `GetPublicKey` request, receive a 32-byte public key
4. Create a client `Keypair`, seal the password to keypunkd's public key
5. Send a `GenerateSeed` request with encrypted password + client public key
6. Receive `SeedGenerated` response with encrypted mnemonic
7. Decrypt the mnemonic using the client `Keypair` — verify it is a valid 12-word BIP39 phrase
8. Verify that in-process messages (without `sender_public_key`) are rejected
9. (Optional) Verify the encrypted blob can be decrypted with the same password

### Build Checklist (Phase 1)

| # | Task | Status |
|---|------|--------|
| 1 | Create `keypunkd` crate with `Cargo.toml` | ✅ Done |
| 2 | Define `KeypunkdRequest` / `KeypunkdResponse` message types (`GetPublicKey`, `GenerateSeed` with encrypted fields) | ✅ Done |
| 3 | Implement `key` module: `generate_seed`, `derive_key`, `encrypt_seed` | ✅ Done |
| 4 | Implement `crypto` module: `Keypair` (X25519 + AES-256-GCM encrypt/decrypt) | ✅ Done |
| 5 | Implement `seed_store` module: `SeedStore` trait, `FilesystemSeedStore`, `InMemorySeedStore` | ✅ Done |
| 6 | Implement `dispatcher` actor: verify_message → deserialize → dispatch (GetPublicKey, GenerateSeed) → serialize | ✅ Done |
| 7 | Implement `services` module: `KeypunkService` wrapping `Recipient<IpcMessage>` for IPC calls | ✅ Done |
| 8 | Implement `usecases` module: `generate_seed` (decrypt → gen → encrypt → persist → encrypt response) | ✅ Done |
| 9 | Implement `errors` module: `GenerateError` enum | ✅ Done |
| 10 | Implement `main.rs`: CLI args, socket bind, keystore bootstrap, tracing init, server loop | ✅ Done |
| 11 | Unit tests: crypto (5), key (2), seed_store (2) | ✅ Done |
| 12 | Integration tests: GetPublicKey, GenerateSeed (encrypted flow), empty password, rejects in-process (4 tests) | ✅ Done |
