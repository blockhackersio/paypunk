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

X25519 key exchange and AES-256-GCM encryption for confidential IPC — the password and returned mnemonic never cross the wire in plaintext.

```rust
/// Long-lived server keypair, generated at daemon startup.
struct KeyStore { /* X25519 secret + public */ }
impl KeyStore {
    fn new() -> Self;
    fn public_key(&self) -> [u8; 32];
    fn decrypt_password(&self, encrypted: &[u8], client_pk: &[u8; 32]) -> Result<String>;
    fn encrypt_mnemonic(&self, mnemonic: &str, client_pk: &[u8; 32]) -> Result<Vec<u8>>;
}

/// Ephemeral client keypair, generated per-request by the caller.
struct CryptoSession { /* X25519 secret + public */ }
impl CryptoSession {
    fn new() -> Self;
    fn public_key(&self) -> [u8; 32];
    fn seal_password(&self, password: &str, server_pk: &[u8; 32]) -> Result<Vec<u8>>;
    fn open_mnemonic(&self, encrypted: &[u8], server_pk: &[u8; 32]) -> Result<String>;
}
```

- Shared secret derived via X25519 DH between the two keypairs.
- AES-256-GCM key derived from shared secret via Blake2b.
- `decrypt_password`: Used by KeyStore to recover the plaintext password for Argon2id seed encryption.
- `encrypt_mnemonic`: Used by KeyStore to encrypt the generated mnemonic back to the requesting client.
- `seal_password`: Used by the client (CryptoSession) to encrypt the password for keypunkd.
- `open_mnemonic`: Used by the client to decrypt the returned mnemonic.

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
keypunkd [--socket-path /path/to/keypunkd.sock] [--data-dir /path/to/data]
```

1. Parse CLI args (`--socket-path`, `--data-dir`)
2. Create `KeyStore` (generates X25519 keypair for IPC encryption)
3. Create `FilesystemSeedStore` pointing at `{data_dir}/seed.enc`
4. Create `Dispatcher` actor with keystore and seed store
5. Bind `UnixListener` at `--socket-path` (default: `$DATA_DIR/keypunkd.sock`)
6. Create `IpcReceiver::new(listener, secret, public)` — shares the KeyStore keypair so the IPC handshake key matches the encryption key
7. Serve connections until Ctrl+C

### Dispatcher Actor

A tactix actor generic over any `SeedStore` implementation:

```rust
struct Dispatcher<S: Storage> {
    keystore: KeyStore,
    seed_store: S,
}
```

1. Receives `IpcMessage` (raw bytes)
2. Deserializes bytes → `KeypunkdRequest` via `postcard`
3. Matches on the request variant:
   - `GetPublicKey` → returns `KeypunkdResponse::PublicKey { key }`
   - `GenerateSeed { encrypted_password, client_public_key }` → calls `handle_generate_seed()`:
     - Decrypts password using `KeyStore::decrypt_password()`
     - Calls `key::generate_seed()` to create 64-byte seed + mnemonic
     - Calls `key::encrypt_seed()` with the recovered password
     - Persists via `SeedStore::write()`
     - Encrypts mnemonic back to client's public key via `KeyStore::encrypt_mnemonic()`
     - Returns `KeypunkdResponse::SeedGenerated { encrypted_mnemonic }`
4. Serializes `KeypunkdResponse` → response bytes via `postcard`
5. Returns bytes through the IPC actor

### Directory Structure

```
keypunkd/
├── SPEC.md              # This file
├── Cargo.toml
└── src/
    ├── lib.rs            # Crate root, re-exports public modules
    ├── main.rs           # CLI entry point, daemon bootstrap
    ├── messages.rs       # KeypunkdRequest, KeypunkdResponse types
    ├── dispatcher.rs     # Tactix actor: deserialize → dispatch → serialize
    ├── key.rs            # Seed generation, BIP39, Argon2id encrypt/decrypt
    ├── crypto.rs         # X25519 KeyStore + CryptoSession for encrypted IPC
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

### Validation Checkpoint

Tests that:
1. Start keypunkd on a temp socket path
2. Connect via `IpcSender` (performs X25519 handshake automatically)
3. Send a `GetPublicKey` request, receive a 32-byte public key
4. Create a `CryptoSession`, seal the password to keypunkd's public key
5. Send a `GenerateSeed` request with encrypted password + client public key
6. Receive `SeedGenerated` response with encrypted mnemonic
7. Decrypt the mnemonic using the CryptoSession — verify it is a valid 12-word BIP39 phrase
8. Verify `seed.enc` exists and is non-empty (filesystem variant)
9. (Optional) Verify the encrypted blob can be decrypted with the same password

### Build Checklist (Phase 1)

| # | Task | Status |
|---|------|--------|
| 1 | Create `keypunkd` crate with `Cargo.toml` | ✅ Done |
| 2 | Define `KeypunkdRequest` / `KeypunkdResponse` message types (`GetPublicKey`, `GenerateSeed` with encrypted fields) | ✅ Done |
| 3 | Implement `key` module: `generate_seed`, `derive_key`, `encrypt_seed` | ✅ Done |
| 4 | Implement `crypto` module: `KeyStore` + `CryptoSession` (X25519 + AES-256-GCM) | ✅ Done |
| 5 | Implement `seed_store` module: `SeedStore` trait, `FilesystemSeedStore`, `InMemorySeedStore` | ✅ Done |
| 6 | Implement `dispatcher` actor: deserialize → dispatch (GetPublicKey, GenerateSeed) → serialize | ✅ Done |
| 7 | Implement `main.rs`: CLI args, socket bind, keystore bootstrap, server loop | ✅ Done |
| 8 | Unit tests: crypto (5), key (2), seed_store (2) | ✅ Done |
| 9 | Integration tests: GetPublicKey, GenerateSeed (encrypted flow), empty password (3 tests) | ✅ Done |
