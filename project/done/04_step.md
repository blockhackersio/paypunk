# Step 4: Ethereum protocol split — EthereumSignerProtocol

## Goal

Split `EthereumProtocol` into two structs: `EthereumProtocol` (keeps `Protocol`
trait impl, requires `T: EthRpcClient`) and `EthereumSignerProtocol` (new, gets
`SignerProtocol` trait impl, no RPC client needed). This mirrors the Zcash split
from Step 3.

## Files to change

### 1. `protocols/ethereum/src/signer.rs` — New file

```rust
use async_trait::async_trait;
use alloy_rlp::Decodable;
use paypunk_types::{
    ArtifactSummary, EthereumArtifactSummary, ProtocolId, SignerProtocol,
};
use crate::protocol::{TxEip1559, TxKind};

pub struct EthereumSignerProtocol;

impl EthereumSignerProtocol {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SignerProtocol for EthereumSignerProtocol {
    fn export_viewing(&self, seed: &[u8; 64], path: &str) -> Result<Vec<u8>, String> {
        use bip32::secp256k1::elliptic_curve::SecretKey;
        use k256::ecdsa::SigningKey;
        let child_key = derive_secp256k1_child(seed, path)?;
        let signing_key = SigningKey::from_slice(&child_key)
            .map_err(|e| format!("invalid key: {e}"))?;
        let verifying_key = signing_key.verifying_key();
        Ok(verifying_key.to_sec1_bytes().to_vec())
    }

    fn parse_artifact(&self, artifact: &[u8]) -> Result<Vec<u8>, String> {
        let tx = TxEip1559::decode(&mut &*artifact)
            .map_err(|e| format!("RLP decode failed: {e}"))?;

        let to = match tx.to {
            TxKind::Call(addr) => addr.to_string(),
            TxKind::Create => "contract_creation".to_string(),
        };

        let amount = format!("{}", tx.value);
        let fee = format!("{}", tx.max_fee_per_gas * tx.gas_limit as u128);

        let summary = ArtifactSummary::Ethereum(EthereumArtifactSummary {
            to,
            amount,
            fee,
            nonce: tx.nonce,
        });

        postcard::to_allocvec(&summary).map_err(|e| format!("serialize summary failed: {e}"))
    }

    fn sign(&self, seed: &[u8; 64], path: &str, artifact: &[u8]) -> Result<Vec<u8>, String> {
        // Reuse the existing sign_transaction_inner logic from EthereumProtocol
        // but without the self.client dependency
        use alloy_rlp::Encodable;
        use k256::ecdsa::{SigningKey, Signature};
        use k256::SecretKey;

        let mut tx = TxEip1559::decode(&mut &*artifact)
            .map_err(|e| format!("RLP decode failed: {e}"))?;

        let child_key = derive_secp256k1_child(seed, path)?;
        let signing_key = SigningKey::from_slice(&child_key)
            .map_err(|e| format!("invalid key: {e}"))?;

        // Sign the transaction hash
        let tx_hash = tx.signature_hash();
        let (sig, recid) = signing_key
            .sign_prehash_recoverable(&tx_hash)
            .map_err(|e| format!("signing failed: {e}"))?;

        tx.set_signature(sig, recid);

        let mut signed = Vec::new();
        tx.encode(&mut signed);
        Ok(signed)
    }
}

fn derive_secp256k1_child(seed: &[u8; 64], path: &str) -> Result<[u8; 32], String> {
    // Same logic as EthereumProtocol::export_viewing key derivation
    // ...
}
```

Note: The exact implementation should mirror the existing `EthereumProtocol`'s
`SignerProtocol` impl methods. The `export_viewing`, `parse_artifact`, and `sign`
methods should be copied from `protocols/ethereum/src/protocol.rs` lines 221-271,
removing only the `self.client` dependency. The `sign_transaction_inner` and
`derive_secp256k1_child` helpers should be moved here too.

### 2. `protocols/ethereum/src/lib.rs`

Add `pub mod signer;` and export `EthereumSignerProtocol`:

```rust
pub mod protocol;
pub mod signer;

pub use protocol::EthereumProtocol;
pub use signer::EthereumSignerProtocol;
```

### 3. `protocols/ethereum/src/protocol.rs`

Keep the `impl<T: EthRpcClient> SignerProtocol for EthereumProtocol<T>` block
unchanged for now. Both `EthereumProtocol` and `EthereumSignerProtocol` will
implement `SignerProtocol` at this stage. The `EthereumProtocol`'s impl will be
removed in a later step (or can stay — it's harmless).

### 4. `protocols/ethereum/Cargo.toml`

Ensure `k256` and `bip32` are in dependencies (they should already be there for
the existing Ethereum signer code). If `alloy-rlp` is used, ensure it's available.

## Acceptance criteria

1. `cargo build --workspace` succeeds.
2. `cargo test --workspace` passes.
3. `cargo fmt --all` produces no changes.
4. `EthereumSignerProtocol` exists as a struct implementing `SignerProtocol`.
5. `EthereumSignerProtocol::new()` takes no arguments (no RPC client).
6. Both `EthereumProtocol` and `EthereumSignerProtocol` implement `SignerProtocol`
   (coexisting, like Zcash in Step 3).

## Context

`EthereumSignerProtocol` is a non-generic struct with no fields. It doesn't need
an RPC client because `SignerProtocol` methods are self-contained:
- `parse_artifact` uses `alloy_rlp` to decode the raw EIP-1559 transaction bytes.
- `sign` uses `k256` for local signing.
- `export_viewing` derives a secp256k1 public key from the seed.

The `EthereumProtocol`'s `SignerProtocol` impl currently uses `self.client` only
in the `chain()` method (which was removed in Step 2). The remaining methods
(`export_viewing`, `parse_artifact`, `sign`) don't use `self.client` either, so the
split is straightforward.

## Verification

```bash
cargo build --workspace
cargo test --workspace
cargo test -p paypunk-chains-ethereum
cargo fmt --all
```

After verification, move this file to `./project/done/04_step.md` and commit with:

```
git add -A && git commit -m "ethereum: add EthereumSignerProtocol without RPC client dependency"
```