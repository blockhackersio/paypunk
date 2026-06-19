# Goal 5: Doc comments on usecases.rs

## Context

The project notes require a minimum of 1-line doc comments on all functions in usecases.rs files. This is a straightforward documentation pass.

Two files need comments:
- `keypunkd/src/usecases.rs` (148 lines, 8 public functions)
- `paypunkd/src/usecases.rs` (196 lines, 16 public functions)

## Current state

### `keypunkd/src/usecases.rs` functions missing doc comments:

| Function | Has doc comment? |
|----------|-----------------|
| `generate_seed()` | No |
| `restore_seed()` | No |
| `decrypt_seed()` | Yes (line 67-69) |
| `validate_mnemonic()` | Yes (line 92-94) |
| `export_viewing_key()` | Yes (line 99-100) |
| `preview_artifact()` | No |
| `sign_artifact()` | No |

### `paypunkd/src/usecases.rs` functions missing doc comments:

| Function | Has doc comment? |
|----------|-----------------|
| `get_keypunk_encryption_key()` | No |
| `generate_seed()` | No |
| `restore_seed()` | No |
| `export_viewing_key()` | No |
| `submit_intent()` | Yes (line 45-46) |
| `approve_signature()` | Yes (line 80-81) |
| `finalize_artifact()` | Yes (line 94) |
| `derive_address()` | Yes (line 103) |
| `validate_address()` | Yes (line 122) |
| `get_balance()` | Yes (line 132) |
| `create_transfer()` | No |
| `get_history()` | No |
| `sync_wallet()` | No |
| `broadcast_transaction()` | No |
| `get_transaction_status()` | No |
| `get_current_block_height()` | No |
| `estimate_fee()` | No |

## Implementation plan

### 1. Add doc comments to `keypunkd/src/usecases.rs`

For each function without a doc comment, add a `///` line before the function. Follow the existing style:

```rust
/// Generate a new BIP39 seed, encrypt it with the user's password, and persist to the seed store.
/// Returns the encrypted mnemonic for the client to display.
pub fn generate_seed(
```

```rust
/// Restore a wallet from an existing BIP39 mnemonic phrase.
/// Validates the mnemonic, derives the seed, encrypts with password, and persists.
pub fn restore_seed(
```

```rust
/// Parse an unsigned artifact into a serialized ArtifactSummary for user preview.
/// Dispatches to the appropriate SignerProtocol based on the protocol ID.
pub fn preview_artifact(
```

```rust
/// Sign an artifact using the decrypted seed.
/// Tries each registered protocol until one successfully signs the artifact.
pub fn sign_artifact(
```

### 2. Add doc comments to `paypunkd/src/usecases.rs`

```rust
/// Forward a GetEncryptionKey request to keypunkd and return its X25519 public key.
pub async fn get_keypunk_encryption_key(
```

```rust
/// Forward a GenerateSeed request to keypunkd with the encrypted password.
/// Returns the encrypted mnemonic from keypunkd.
pub async fn generate_seed(
```

```rust
/// Forward a RestoreSeed request to keypunkd with the encrypted mnemonic and password.
pub async fn restore_seed(
```

```rust
/// Forward an ExportViewingKey request to keypunkd to derive viewing key material
/// for the given protocol and account index.
pub async fn export_viewing_key(
```

For the `todo!()` stubs, add doc comments explaining what they will do:

```rust
/// Create a transfer for the given protocol and account.
/// TODO: Requires PCZT pipeline — not yet implemented.
pub async fn create_transfer(
```

```rust
/// Fetch transaction history for the given protocol and account.
/// TODO: Requires Page/HistoryEntry types and chain backend — not yet implemented.
pub async fn get_history(
```

```rust
/// Sync the wallet state with the blockchain for the given protocol and account.
/// TODO: Requires LSP/lightwalletd connection — not yet implemented.
pub async fn sync_wallet(
```

```rust
/// Finalize and broadcast a signed transaction to the network.
/// Returns the transaction hash.
pub fn broadcast_transaction(
```

```rust
/// Query the on-chain status of a transaction by its ID.
/// TODO: Requires lightwalletd/RPC client — not yet implemented.
pub async fn get_transaction_status(
```

```rust
/// Get the current block height from the blockchain.
/// TODO: Requires lightwalletd/RPC client — not yet implemented.
pub async fn get_current_block_height(
```

```rust
/// Estimate the fee for a transfer to the given address with the given amount and optional memo.
/// TODO: Requires TransactionProposer + chain fee estimation — not yet implemented.
pub async fn estimate_fee(
```

### 3. Verify with `cargo doc`

Run `cargo doc --no-deps -p keypunkd -p paypunkd` and verify no warnings about missing doc comments.

## Files to modify

- `keypunkd/src/usecases.rs` — add doc comments
- `paypunkd/src/usecases.rs` — add doc comments

## Tests

No functional tests needed for this goal. Verification is via `cargo doc`.

### Verification command

```bash
cargo doc --no-deps -p keypunkd -p paypunkd 2>&1 | grep -i "warning\|missing docs"
```

Should produce no warnings.

## Acceptance criteria

- Every public function in both files has a `///` doc comment explaining purpose and context
- `cargo doc` builds without warnings for these crates
