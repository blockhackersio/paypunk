# Step 1: Types & data shape — Move enums, add types, change ArtifactSummary to enum

## Goal

Move `KeypunkdRequest`/`KeypunkdResponse` from `keypunkd/src/messages.rs` into
`types/src/lib.rs`. Add `chain_id: ChainId` to `PreviewArtifact`. Add
`SubmitIntentResult`, `OutputEntry`, `ZcashArtifactSummary`, `EthereumArtifactSummary`.
Change `ArtifactSummary` from a flat struct to a protocol-specific enum. Update all
imports, match arms, construction sites, and deserialization sites across the
workspace.

## Files to change

### 1. `types/src/lib.rs`

**Add new types** before the `ArtifactSummary` section (around line 55):

```rust
/// A single output entry in the artifact summary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutputEntry {
    pub address: String,
    pub amount: String,
}

/// Zcash-specific artifact summary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ZcashArtifactSummary {
    pub outputs: Vec<OutputEntry>,
    pub fee: String,
}

/// Ethereum-specific artifact summary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EthereumArtifactSummary {
    pub to: String,
    pub amount: String,
    pub fee: String,
    pub nonce: u64,
}
```

**Replace the existing `ArtifactSummary` struct** (lines 57-65) with:

```rust
/// Protocol-specific artifact summary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ArtifactSummary {
    Zcash(ZcashArtifactSummary),
    Ethereum(EthereumArtifactSummary),
}
```

**Add `SubmitIntentResult`** after the `ArtifactSummary` section:

```rust
/// Result of submit_intent for the API/TUI layer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SubmitIntentResult {
    /// Keypunkd mode: preview to display in TUI, then approve_signature.
    SignablePreview {
        raw_artifact: Vec<u8>,
        parsed_summary: Vec<u8>,
        keypunkd_signature: Vec<u8>,
        keypunkd_public_key: [u8; 32],
    },
    /// Signer mode: signed artifact, skip preview and password.
    SignatureApproved {
        signed_artifact: Vec<u8>,
    },
}
```

**Move `KeypunkdRequest` and `KeypunkdResponse`** from `keypunkd/src/messages.rs`
into `types/src/lib.rs`. Place them after the `SubmitIntentResult` section. Add
`chain_id` to the `PreviewArtifact` variant:

```rust
#[derive(Debug, Serialize, Deserialize)]
pub enum KeypunkdRequest {
    GetEncryptionKey,
    GenerateSeed {
        encrypted_password: Vec<u8>,
        client_public_key: [u8; 32],
    },
    RestoreSeed {
        encrypted_mnemonic: Vec<u8>,
        encrypted_password: Vec<u8>,
        client_public_key: [u8; 32],
    },
    PreviewArtifact {
        raw_artifact: Vec<u8>,
        protocol: ProtocolId,
        chain_id: ChainId,          // NEW
        derivation_path: String,
    },
    AuthorizeArtifact {
        encrypted_payload: Vec<u8>,
        ephemeral_public_key: [u8; 32],
        derivation_path: String,
    },
    ExportViewingKey {
        encrypted_password: Vec<u8>,
        client_public_key: [u8; 32],
        protocol: ProtocolId,
        derivation_path: String,
    },
    HasSeed,
    VerifyPassword {
        encrypted_password: Vec<u8>,
        client_public_key: [u8; 32],
    },
    BulkExportViewingKeys {
        encrypted_password: Vec<u8>,
        client_public_key: [u8; 32],
        paths: Vec<(ProtocolId, String)>,
    },
    ExportMnemonic {
        encrypted_password: Vec<u8>,
        client_public_key: [u8; 32],
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum KeypunkdResponse {
    EncryptionKey { key: [u8; 32] },
    SeedGenerated { encrypted_mnemonic: Vec<u8> },
    SeedRestored,
    ArtifactPreview {
        raw_artifact: Vec<u8>,
        parsed_summary: Vec<u8>,
        signature: Vec<u8>,
        keypunkd_public_key: [u8; 32],
    },
    ArtifactAuthorized { signed_artifact: Vec<u8> },
    ViewingKey { key: Vec<u8> },
    HasSeed { exists: bool },
    PasswordVerified,
    ViewingKeys { keys: Vec<(ProtocolId, String, Vec<u8>)> },
    MnemonicExported { encrypted_mnemonic: Vec<u8> },
    Error { message: String },
}
```

### 2. `keypunkd/src/messages.rs`

Replace the entire file content with:

```rust
pub use paypunk_types::{KeypunkdRequest, KeypunkdResponse};
```

### 3. `keypunkd/src/keypunk.rs`

In the `handle_request` method (around line 344), the `PreviewArtifact` match arm
currently destructures `raw_artifact, protocol, derivation_path`. Update it to
include `chain_id`:

```rust
KeypunkdRequest::PreviewArtifact {
    raw_artifact,
    protocol,
    chain_id: _,   // ignored in keypunkd mode
    derivation_path,
} => self.preview_artifact(raw_artifact, protocol, derivation_path, sender_public_key),
```

Also update the `preview_artifact` method signature (around line 92) — no change
needed to the method params, it already accepts `raw_artifact, protocol, derivation_path, sender_public_key`.

### 4. `keypunkd/src/services.rs`

Update the `preview_artifact` method to accept `chain_id` and pass it in the
`KeypunkdRequest::PreviewArtifact`:

```rust
pub async fn preview_artifact(
    &self,
    raw_artifact: Vec<u8>,
    protocol: ProtocolId,
    chain_id: ChainId,
    derivation_path: String,
) -> Result<KeypunkdResponse, String> {
    self.send(KeypunkdRequest::PreviewArtifact {
        raw_artifact,
        protocol,
        chain_id,
        derivation_path,
    })
    .await
}
```

Add `use paypunk_types::ChainId;` to the imports if not already present.

### 5. `keypunkd/src/keypunkd.rs`

No change needed — `KeypunkdRequest` is imported from `crate::messages` which now
re-exports from `paypunk_types`.

### 6. `keypunkd/tests/generate_seed_test.rs`

The import `use keypunkd::messages::{KeypunkdRequest, KeypunkdResponse};` will
still work because `keypunkd::messages` re-exports from `paypunk_types`.

### 7. `paypunkd/src/usecases.rs`

Update the `submit_intent` function (around lines 90-117):

- Import `paypunk_types::ChainId` instead of referencing `keypunkd::messages::KeypunkdResponse`
- Get the `chain_id` from `protocol.chain_id()` (the `Protocol` trait method)
- Pass `chain_id` to `keypunk_service.preview_artifact()`
- Update the match on `KeypunkdResponse` — use `paypunk_types::KeypunkdResponse` instead of `keypunkd::messages::KeypunkdResponse`

Change the import from:
```rust
use keypunkd::messages::KeypunkdResponse;
```
to:
```rust
use paypunk_types::KeypunkdResponse;
```

The `preview_artifact` call changes from:
```rust
keypunk_service.preview_artifact(raw_artifact, protocol_id, derivation_path.to_string())
```
to:
```rust
let chain_id = protocol.chain_id();
keypunk_service.preview_artifact(raw_artifact, protocol_id, chain_id, derivation_path.to_string())
```

### 8. `protocols/zcash/src/protocol.rs`

Update the `parse_artifact` method in `impl SignerProtocol for ZcashProtocol`
(around lines 108-129). Change the `ArtifactSummary` construction from struct to
enum:

```rust
fn parse_artifact(&self, artifact: &[u8]) -> Result<Vec<u8>, String> {
    let pczt = pczt::Pczt::parse(artifact).map_err(|e| format!("PCZT parse failed: {e:?}"))?;

    let (value_sum, negative) = pczt.orchard().value_sum();
    let fee = if *negative { 0u64 } else { *value_sum };

    let summary = ArtifactSummary::Zcash(ZcashArtifactSummary {
        outputs: vec![],  // hardcoded stubs for now (real extraction in Stage 3)
        fee: fee.to_string(),
    });

    postcard::to_allocvec(&summary).map_err(|e| format!("serialize summary failed: {e}"))
}
```

Update the import to use the new types:
```rust
use paypunk_types::{ArtifactSummary, ZcashArtifactSummary};
```

### 9. `protocols/ethereum/src/protocol.rs`

Update the `parse_artifact` method in `impl SignerProtocol for EthereumProtocol`
(around lines 244-266). Change the `ArtifactSummary` construction from struct to
enum:

```rust
fn parse_artifact(&self, artifact: &[u8]) -> Result<Vec<u8>, String> {
    let tx: TxEip1559 =
        alloy_rlp::decode_exact(artifact).map_err(|e| format!("RLP decode failed: {e}"))?;

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
```

Update the import to use the new types:
```rust
use paypunk_types::{ArtifactSummary, EthereumArtifactSummary};
```

Also update the `test_parse_artifact` test (around line 400) to match on the enum:

```rust
let summary: ArtifactSummary = postcard::from_bytes(&parsed).expect("should deserialize");
match &summary {
    ArtifactSummary::Ethereum(eth) => {
        assert_eq!(eth.to, "0x0000000000000000000000000000000000000000");
        assert_eq!(eth.amount, "1000000000000000000");
    }
    _ => panic!("expected Ethereum summary"),
}
```

### 10. `tui/src/api/real.rs`

Update the `submit_send_review` method (around lines 400-420) where
`parsed_summary` is deserialized. Change from struct field access to enum matching:

```rust
if let Ok(summary) = postcard::from_bytes::<ArtifactSummary>(&parsed_summary) {
    match &summary {
        ArtifactSummary::Zcash(zcash) => {
            let total = zcash.fee.parse::<u128>().unwrap_or(0);
            SendReviewData {
                to_address: "Zcash transfer".to_string(),
                amount: "0".to_string(),
                fee_estimate: zcash.fee.clone(),
                total_amount: total.to_string(),
                chain_id: input.chain_id,
                nonce: 0,
            }
        }
        ArtifactSummary::Ethereum(eth) => {
            let total = eth.amount.parse::<u128>().unwrap_or(0)
                + eth.fee.parse::<u128>().unwrap_or(0);
            SendReviewData {
                to_address: eth.to.clone(),
                amount: eth.amount.clone(),
                fee_estimate: eth.fee.clone(),
                total_amount: total.to_string(),
                chain_id: input.chain_id,
                nonce: eth.nonce,
            }
        }
    }
}
```

### 11. `cli/src/main.rs`

Update the `submit_intent_flow` function (around line 722) where `parsed_summary`
is deserialized. Change from struct field access to enum matching:

```rust
let summary: ArtifactSummary = postcard::from_bytes(&parsed_summary)
    .map_err(|e| format!("Failed to parse summary: {e}"))?;
match &summary {
    ArtifactSummary::Zcash(zcash) => {
        println!("  Fee: {} zatoshis", zcash.fee);
        // ... existing output logic adapted to Zcash variant
    }
    ArtifactSummary::Ethereum(eth) => {
        println!("  To: {}", eth.to);
        println!("  Amount: {} wei", eth.amount);
        println!("  Fee: {} wei", eth.fee);
        println!("  Nonce: {}", eth.nonce);
    }
}
```

### 12. `tests/tests/integration_test.rs`

Update `test_eth_send_full_flow` (around lines 390-391) where `parsed_summary`
is deserialized. Change from:

```rust
let summary: ArtifactSummary =
    postcard::from_bytes(&parsed_summary).expect("should deserialize");
assert_eq!(summary.protocol, ProtocolId::Ethereum);
assert_eq!(summary.to, "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045");
```

to:

```rust
let summary: ArtifactSummary =
    postcard::from_bytes(&parsed_summary).expect("should deserialize");
match &summary {
    ArtifactSummary::Ethereum(eth) => {
        assert_eq!(eth.to, "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045");
    }
    _ => panic!("expected Ethereum summary"),
}
```

Also check if `test_eth_balance_via_mock_rpc`, `test_eth_balance_zero` use
`ArtifactSummary` — if they do, update them similarly.

### 13. `types/src/lib.rs` — Update `ProtocolId` import

Ensure `ProtocolId` is imported/defined before `KeypunkdRequest` uses it. It
already is (line 6-10).

### 14. `types/src/lib.rs` — Update `ChainId` import

Ensure `ChainId` is imported/defined before `KeypunkdRequest` uses it. It's
already imported via `pub use caip::ChainId;`.

## Acceptance criteria

1. `cargo build --workspace` succeeds with no errors.
2. `cargo test --workspace` passes — all existing tests pass.
3. `cargo fmt --all` produces no changes.
4. All `KeypunkdRequest::PreviewArtifact` match arms include `chain_id` (ignored).
5. `ArtifactSummary` is an enum with `Zcash` and `Ethereum` variants.
6. `keypunkd::messages` re-exports `KeypunkdRequest` and `KeypunkdResponse` from
   `paypunk_types`.
7. `SubmitIntentResult` is defined in `paypunk_types` (not yet consumed — that
   happens in Stage 6).

## Context

The `ArtifactSummary` currently looks like this:
```rust
pub struct ArtifactSummary {
    pub to: String,
    pub amount: String,
    pub fee: String,
    pub nonce: u64,
    pub memo: Option<String>,
    pub protocol: ProtocolId,
}
```

After this change, it becomes a protocol-specific enum. The `protocol` field is no
longer needed because the enum variant carries the protocol. The `memo` field is
removed because it's not extractable from either PCZT or RLP artifacts.

The `chain_id` field on `PreviewArtifact` is needed for the signer app to
auto-configure which protocol to use. In keypunkd mode, it's received but ignored
(keypunkd already knows the protocol from its registered protocols).

The `OutputEntry` type is new — it represents a single output in a Zcash
transaction (recipient address + amount). It's currently unused but will be used
in Stage 3 when `ZcashSignerProtocol::parse_artifact` extracts real output data
from the PCZT.

## Verification

```bash
cargo build --workspace
cargo test --workspace
cargo fmt --all
```

After verification, move this file to `./project/done/01_step.md` and commit with:

```
git add -A && git commit -m "types: move KeypunkdRequest/KeypunkdResponse to types, add chain_id, SubmitIntentResult, change ArtifactSummary to enum"
```