# TransactionBuilder — Implementation Plan

## Motivation

The current `Protocol::propose_and_build` takes `&dyn WalletRepository`, but Zcash's shielded note model requires rich per-note data (commitments, merkle paths, nullifiers, diversifiers) that has no analogue in simpler chains. This data lives in `zcash_client_sqlite::WalletDb`. Previous attempts to work around this (downcasting via `as_any`, opaque `get_resource_details`, protocol-owned WalletDb) all had architectural drawbacks.

## Solution: TransactionBuilder trait

A new `TransactionBuilder` trait separates the concern of "building unsigned transactions from chain data" from both `WalletRepository` (pure data access) and `Protocol` (stateless crypto operations).

```
WalletRepository     — pure data access: balance, sync state, store transactions
TransactionBuilder   — builds unsigned transactions: input selection, fee computation, output construction
Protocol             — stateless crypto: prove, sign, finalize transactions
```

## Trait Definitions

### `types/src/lib.rs`

```rust
/// Builds unsigned transactions for a specific chain.
///
/// Holds chain-specific primitives (e.g. WalletDb for Zcash, RPC client for
/// Ethereum). The implementation knows how to select inputs, compute fees,
/// and construct the unsigned transaction.
pub trait TransactionBuilder: Send + Sync {
    /// Build an unsigned transaction.
    ///
    /// Returns protocol-specific serialized bytes (e.g. a PCZT for Zcash,
    /// an unsigned RLP transaction for Ethereum).
    fn build_unsigned_transaction(
        &self,
        protocol_id: ProtocolId,
        account: u32,
        to: &str,
        amount: u64,
        memo: Option<&str>,
    ) -> Result<Vec<u8>, String>;
}
```

### Updated `Protocol` trait

```rust
pub trait Protocol: Send + Sync {
    fn protocol_id(&self) -> ProtocolId;
    fn derive_address(&self, public_key: &[u8], index: u32) -> Result<String, String>;

    /// Build an unsigned transaction using the given builder.
    ///
    /// Default implementation delegates to
    /// `builder.build_unsigned_transaction(...)`. Protocols that need
    /// additional processing can override.
    fn build_transaction(
        &self,
        builder: &dyn TransactionBuilder,
        account: u32,
        to: &str,
        amount: u64,
        memo: Option<&str>,
    ) -> Result<Vec<u8>, String> {
        builder.build_unsigned_transaction(self.protocol_id(), account, to, amount, memo)
    }

    fn prove_transaction(&self, transaction: &[u8]) -> Result<Vec<u8>, String>;
    fn finalize_transaction(&self, transaction: &[u8]) -> Result<Vec<u8>, String>;
}
```

### `WalletRepository` — unchanged

```rust
pub trait WalletRepository: Send + Sync {
    fn get_balance(&self, account: u32) -> Result<Balance, String>;
    fn get_spendable_resources(&self, account: u32) -> Result<Vec<Vec<u8>>, String>;
    fn mark_resources_spent(&self, account: u32, txid: &str) -> Result<(), String>;
    fn store_transaction(&self, account: u32, txid: &str, raw_tx: &[u8]) -> Result<(), String>;
}
```

No `as_any`, no downcasting, no changes.

## Implementation: Zcash

### New file: `protocols/zcash/src/transaction_builder.rs`

```rust
pub struct ZcashTransactionBuilder {
    wallet_db: Arc<Mutex<WalletDb<SystemClock, LocalNetwork>>>,
    params: LocalNetwork,
}

impl ZcashTransactionBuilder {
    pub fn new(wallet_db: WalletDb<SystemClock, LocalNetwork>, params: LocalNetwork) -> Self {
        Self {
            wallet_db: Arc::new(Mutex::new(wallet_db)),
            params,
        }
    }

    pub fn wallet_db(&self) -> &Arc<Mutex<WalletDb<SystemClock, LocalNetwork>>> {
        &self.wallet_db
    }
}

impl TransactionBuilder for ZcashTransactionBuilder {
    fn build_unsigned_transaction(
        &self,
        protocol_id: ProtocolId,
        account: u32,
        to: &str,
        amount: u64,
        memo: Option<&str>,
    ) -> Result<Vec<u8>, String> {
        // 1. Lock WalletDb
        // 2. Parse recipient address
        // 3. Build TransactionRequest with optional memo
        // 4. Create MultiOutputChangeStrategy (Orchard, Zip317 fees)
        // 5. Call propose_transfer with GreedyInputSelector
        // 6. Call create_pczt_from_proposal
        // 7. Return serialized PCZT bytes
    }
}
```

### `protocols/zcash/src/protocol.rs` — stays a unit struct

```rust
pub struct ZcashProtocol;

impl Protocol for ZcashProtocol {
    // build_transaction uses default impl (delegates to builder)
    // prove_transaction — as before (Prover role)
    // finalize_transaction — as before (SpendFinalizer + TransactionExtractor)
}
```

### `protocols/zcash/src/lib.rs`

```rust
pub mod address;
pub mod protocol;
pub mod repository;
pub mod transaction_builder;
```

### `protocols/zcash/Cargo.toml`

Keep `zcash_client_sqlite`, `zcash_client_backend`, `rusqlite`, `secrecy` as regular dependencies (already promoted).

## Implementation: ZcashWalletRepository

### `protocols/zcash/src/repository.rs` — already created

Wraps `Arc<Mutex<WalletDb>>`, implements `WalletRepository`'s 4 methods. No changes needed.

## Integration: paypunkd usecase

The `CreateTransfer` usecase in paypunkd (Step 5 in SPEC) wires the components:

```rust
async fn create_transfer(
    protocol: &dyn Protocol,
    builder: &dyn TransactionBuilder,
    signer: &dyn SignerProtocol,  // via IPC to keypunkd
    repository: &dyn WalletRepository,
    account: u32,
    to: &str,
    amount: u64,
    memo: Option<&str>,
) -> Result<String, String> {
    // 1. Build unsigned transaction
    let unsigned = protocol.build_transaction(builder, account, to, amount, memo)?;

    // 2. Prove (ZK proofs for Zcash, no-op for others)
    let proven = protocol.prove_transaction(&unsigned)?;

    // 3. Sign via keypunkd IPC
    let signed = signer.sign_transaction(seed, account, &proven)?;

    // 4. Finalize
    let raw_tx = protocol.finalize_transaction(&signed)?;

    // 5. Compute txid and store
    let txid = "...";
    repository.store_transaction(account, &txid, &raw_tx)?;

    Ok(txid)
}
```

## Testing

### New test: `tests/tests/pczt_wallet_pipeline.rs`

Full pipeline with in-memory WalletDb:

```
1. Create in-memory WalletDb (temp dir + init_wallet_db)
2. Create account from seed
3. Insert test Orchard notes into WalletDb
4. Create ZcashTransactionBuilder with WalletDb + LocalNetwork params
5. Create ZcashWalletRepository wrapping same Arc<Mutex<WalletDb>>
6. Call ZcashProtocol::build_transaction(&builder, ...) → PCZT bytes
7. Call prove_transaction → proven PCZT
8. Call sign_transaction (SignerProtocol) → signed PCZT
9. Call finalize_transaction → raw tx
10. Verify well-formed Orchard bundle
```

### Existing tests — minor updates

- `tests/tests/pczt_test.rs`: `ZcashProtocol` → `ZcashProtocol::default()` (already done)
- `tests/tests/integration_test.rs`: `ZcashProtocol` → `ZcashProtocol::default()` (already done)
- `paypunkd/src/main.rs`: `ZcashProtocol` → `ZcashProtocol::default()`
- `keypunkd/src/main.rs`: `ZcashProtocol` → `ZcashProtocol::default()`

## Build Sequence

| # | Step | Files |
|---|------|-------|
| 1 | Add `TransactionBuilder` trait to `types/src/lib.rs` | `types/src/lib.rs` |
| 2 | Rename `propose_and_build` → `build_transaction` on `Protocol` trait | `types/src/lib.rs` |
| 3 | Create `ZcashTransactionBuilder` | `protocols/zcash/src/transaction_builder.rs` |
| 4 | Update `ZcashProtocol` to use new trait (stay stateless) | `protocols/zcash/src/protocol.rs` |
| 5 | Update `ZcashWalletRepository` (no downcasting needed) | `protocols/zcash/src/repository.rs` |
| 6 | Update call sites for `ZcashProtocol` construction | `paypunkd/src/main.rs`, `keypunkd/src/main.rs` |
| 7 | Add integration test with in-memory WalletDb | `tests/tests/pczt_wallet_pipeline.rs` |
| 8 | Update existing test crate dependencies | `tests/Cargo.toml` |
| 9 | Build, lint, test | — |
