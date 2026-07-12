# Step 2: Remove `chain()` from `SignerProtocol` trait

## Goal

Remove `async fn chain(&self) -> ChainId` from the `SignerProtocol` trait and
remove its implementations from `ZcashProtocol` and `EthereumProtocol`. The
`chain_id` is now provided via the `chain_id` field in `KeypunkdRequest::PreviewArtifact`
(added in Step 1) and via `Protocol::chain_id()` on the paypunkd side.

## Files to change

### 1. `types/src/lib.rs`

Remove `async fn chain(&self) -> ChainId;` from the `SignerProtocol` trait.

The trait should become:

```rust
#[async_trait::async_trait]
pub trait SignerProtocol: Send + Sync {
    fn export_viewing(&self, seed: &[u8; 64], path: &str) -> Result<Vec<u8>, String>;
    fn parse_artifact(&self, artifact: &[u8]) -> Result<Vec<u8>, String>;
    fn sign(&self, seed: &[u8; 64], path: &str, artifact: &[u8]) -> Result<Vec<u8>, String>;
}
```

Note: After removing the only `async fn`, the `#[async_trait::async_trait]`
attribute is still needed because the trait is `Send + Sync` and `async_trait`
provides the boxing. Keep it.

### 2. `protocols/zcash/src/protocol.rs`

Remove the `async fn chain()` method from `impl SignerProtocol for ZcashProtocol`
(around lines 82-86). The method currently looks like:

```rust
async fn chain(&self) -> ChainId {
    let network = match self.network_type {
        NetworkType::Main => "mainnet",
        NetworkType::Test => "testnet",
        NetworkType::Regtest => "regtest",
    };
    ChainId {
        namespace: "zcash".to_string(),
        reference: network.to_string(),
    }
}
```

Remove the entire method body. The `impl SignerProtocol for ZcashProtocol` block
should now only have `export_viewing`, `parse_artifact`, and `sign`.

### 3. `protocols/ethereum/src/protocol.rs`

Remove the `async fn chain()` method from `impl<T: EthRpcClient> SignerProtocol for EthereumProtocol<T>`
(around lines 222-229). The method currently looks like:

```rust
async fn chain(&self) -> ChainId {
    let chain_id = self.client.get_chain_id().await.unwrap_or(1);
    ChainId {
        namespace: "eip155".to_string(),
        reference: chain_id.to_string(),
    }
}
```

Remove the entire method body.

### 4. `protocols/ethereum/src/protocol.rs` — Update `test_chain_id` test

The test at ~line 393 (`test_chain_id`) currently calls `protocol.chain().await`
which tests the removed `SignerProtocol::chain()` method. Replace it with a test
that exercises `Protocol::chain_id()` instead:

```rust
#[test]
fn test_chain_id() {
    let protocol = EthereumProtocol::new(MockRpcClient::new(0, 0));
    let chain = protocol.chain_id();
    assert_eq!(chain.namespace, "eip155");
    assert_eq!(chain.reference, "1");
}
```

Note: `Protocol::chain_id()` is sync (not async), so the test no longer needs
`#[tokio::test]`. Change it to `#[test]`.

### 5. Check for any other references to `SignerProtocol::chain()` or `.chain()`

Run a grep across the workspace for any remaining calls to `.chain()` on
`SignerProtocol` trait objects. There should be none after the test is updated.

## Acceptance criteria

1. `cargo build --workspace` succeeds.
2. `cargo test --workspace` passes — all tests including the updated `test_chain_id`.
3. `cargo fmt --all` produces no changes.
4. `SignerProtocol` trait has exactly 3 methods: `export_viewing`, `parse_artifact`, `sign`.
5. No `chain()` method exists on either `ZcashProtocol` or `EthereumProtocol`'s
   `SignerProtocol` impl.
6. The Ethereum test `test_chain_id` uses `Protocol::chain_id()` (sync) instead of
   `SignerProtocol::chain()` (async).

## Context

The `chain()` method on `SignerProtocol` was dead code — only called in the
Ethereum test, never in production. The `chain_id` field added to
`KeypunkdRequest::PreviewArtifact` in Step 1 provides the same information at the
IPC level. On the paypunkd side, `Protocol::chain_id()` is the canonical way to
get the chain identifier.

Removing `chain()` from `SignerProtocol` simplifies the trait and eliminates the
asymmetry between `SignerProtocol::chain()` (async) and `Protocol::chain_id()`
(sync). It also removes the only method that required the RPC client in
`EthereumProtocol`'s `SignerProtocol` impl, which is important for the
`EthereumSignerProtocol` split in Stage 4.

## Verification

```bash
cargo build --workspace
cargo test --workspace
cargo test -p paypunk-chains-ethereum  # ensure the updated test_chain_id passes
cargo fmt --all
```

After verification, move this file to `./project/done/02_step.md` and commit with:

```
git add -A && git commit -m "signer: remove chain() from SignerProtocol trait, use Protocol::chain_id() instead"
```