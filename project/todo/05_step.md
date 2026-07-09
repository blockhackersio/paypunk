# Step 5: Remove SignerProtocol from ZcashProtocol, migrate keypunkd

## Goal

Remove the `impl SignerProtocol for ZcashProtocol` block from `protocol.rs`.
Remove `sign_transaction_inner` and `KeyRef` from `ZcashProtocol` (they now live
in `ZcashSignerProtocol`). Update `keypunkd/src/run.rs` to use
`ZcashSignerProtocol` instead of `ZcashProtocol` for protocol registration.

## Files to change

### 1. `protocols/zcash/src/protocol.rs`

**Remove the `impl SignerProtocol for ZcashProtocol` block** (the entire block
including `export_viewing`, `parse_artifact`, `sign`, and `sign_transaction_inner`).

**Remove `KeyRef` enum** (the private enum used inside `sign_transaction_inner`).

**Keep `impl Protocol for ZcashProtocol`** unchanged. `ZcashProtocol` is now
wallet-only — it implements `Protocol` but not `SignerProtocol`.

**Ensure `crate::common` imports** are still used for any remaining shared helpers
in the `Protocol` impl (e.g., `derivation_path` uses `ZCASH_COIN_TYPE`).

### 2. `keypunkd/src/run.rs`

Update the protocol registration for Zcash. Currently (around line 59-66):

```rust
protocols.register(
    ProtocolId::Zcash,
    Box::new(paypunk_chains_zcash::protocol::ZcashProtocol::new(
        to_local_params(params, network_type),
        network_type,
        None,
        None,
        None,
    )),
);
```

Change to:

```rust
protocols.register(
    ProtocolId::Zcash,
    Box::new(paypunk_chains_zcash::signer::ZcashSignerProtocol::new(
        to_local_params(params, network_type),
        network_type,
    )),
);
```

`ZcashSignerProtocol::new()` takes only `params` and `network_type` — no wallet
DB, scan actor, or lightwalletd host.

### 3. `keypunkd/src/run.rs` — Also update Ethereum registration

Similarly, update the Ethereum protocol registration to use `EthereumSignerProtocol`
instead of `EthereumProtocol::new(())`:

```rust
protocols.register(
    ProtocolId::Ethereum,
    Box::new(paypunk_chains_ethereum::signer::EthereumSignerProtocol::new()),
);
```

This eliminates the `EthereumProtocol::new(())` hack (unit type as RPC client).

### 4. `tests/tests/pczt_test.rs`

Update `test_orchard_shielded_pczt_full_pipeline` (around line 134). Currently it
creates a `ZcashProtocol` and calls `protocol.sign()`. Change to use
`ZcashSignerProtocol`:

```rust
use paypunk_chains_zcash::signer::ZcashSignerProtocol;

let signer = ZcashSignerProtocol::new(params.clone(), network_type);
let signed_bytes = signer.sign(&seed, path, &proven_bytes)
    .expect("signing should succeed");
```

The `protocol.finalize()` call (line 137) stays on `ZcashProtocol` since
`finalize` is a `Protocol` method, not a `SignerProtocol` method.

### 5. `tests/tests/integration_test.rs`

Update the `TestBuilder::build()` method (around line 100-150) where keypunkd
protocols are registered. Change from `ZcashProtocol` to `ZcashSignerProtocol` and
from `EthereumProtocol::new(())` to `EthereumSignerProtocol::new()`:

```rust
// keypunkd side
keypunkd_protocols.register(
    ProtocolId::Zcash,
    Box::new(paypunk_chains_zcash::signer::ZcashSignerProtocol::new(
        params, network_type,
    )),
);
keypunkd_protocols.register(
    ProtocolId::Ethereum,
    Box::new(paypunk_chains_ethereum::signer::EthereumSignerProtocol::new()),
);
```

The paypunkd side still uses `ZcashProtocol` and `EthereumProtocol` (with real
mock client) — those are unchanged.

### 6. Check for other references to `ZcashProtocol` as SignerProtocol

Run a grep for any other code that uses `ZcashProtocol` for signing. The
`pczt_test.rs` and `integration_test.rs` are the only ones. If any CLI code
directly constructs a `ZcashProtocol` for signing, update it.

## Acceptance criteria

1. `cargo build --workspace` succeeds.
2. `cargo test --workspace` passes — all tests including pczt_test.rs and
   integration_test.rs.
3. `cargo test -p tests` passes — the integration tests use the new signer types.
4. `cargo fmt --all` produces no changes.
5. `ZcashProtocol` no longer implements `SignerProtocol`.
6. `keypunkd` uses `ZcashSignerProtocol` and `EthereumSignerProtocol` for signing.
7. `EthereumProtocol::new(())` is no longer used in keypunkd.

## Context

`ZcashProtocol` is now a pure wallet-side struct: it implements `Protocol` (build,
broadcast, balance, etc.) but not `SignerProtocol`. `ZcashSignerProtocol` is the
sole `SignerProtocol` impl for Zcash.

`keypunkd` only needs `SignerProtocol` — it does signing, previewing, and key
export. It never calls `Protocol` methods. So switching to the lighter
`ZcashSignerProtocol` and `EthereumSignerProtocol` is correct and reduces the
keypunkd's dependency surface.

The `sign_transaction_inner` and `KeyRef` code in `ZcashSignerProtocol` (from
Step 3) is the only copy now. No duplication.

## Verification

```bash
cargo build --workspace
cargo test --workspace
cargo test -p tests
cargo fmt --all
```

After verification, move this file to `./project/done/05_step.md` and commit with:

```
git add -A && git commit -m "keypunkd: migrate to ZcashSignerProtocol and EthereumSignerProtocol, remove SignerProtocol from ZcashProtocol"
```