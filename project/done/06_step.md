# Step 6: paypunkd + api signer-aware flow

## Goal

Update `paypunkd`'s `submit_intent` usecase to return `KeypunkdResponse` directly
(instead of a 4-tuple). Update the `paypunkd` handler to map `KeypunkdResponse`
variants to `PaypunkdResponse` variants. Update `api` to return `SubmitIntentResult`
enum instead of a 4-tuple.

## Files to change

### 1. `paypunkd/src/usecases.rs`

Change the `submit_intent` function signature and body. Currently it returns
`Result<(Vec<u8>, Vec<u8>, Vec<u8>, [u8; 32]), String>` (a 4-tuple). Change it to
return `Result<KeypunkdResponse, String>`:

```rust
pub async fn submit_intent(
    keypunk_service: &KeypunkService,
    protocols: &ProtocolService,
    intent: &Intent,
    derivation_path: &str,
) -> Result<KeypunkdResponse, String> {
    let protocol_id = match intent {
        Intent::Zcash(_) => ProtocolId::Zcash,
        Intent::Ethereum(_) => ProtocolId::Ethereum,
    };

    let protocol = protocols.get(protocol_id)?;
    let raw_artifact = protocol.build(intent).await?;
    let chain_id = protocol.chain_id();

    keypunk_service
        .preview_artifact(raw_artifact, protocol_id, chain_id, derivation_path.to_string())
        .await
}
```

Remove the match on `KeypunkdResponse::ArtifactPreview` that extracted the
4-tuple — the caller (paypunkd handler) will do the matching.

Import `KeypunkdResponse` if not already:
```rust
use paypunk_types::KeypunkdResponse;
```

### 2. `paypunkd/src/paypunkd.rs`

Update the `submit_intent` handler in the `impl Handler<IpcMessage> for Paypunkd`
(or `impl Paypunkd`) to match on `KeypunkdResponse` and map to the appropriate
`PaypunkdResponse`:

```rust
async fn submit_intent(
    &self,
    intent: paypunk_types::Intent,
    derivation_path: String,
) -> PaypunkdResponse {
    info!("handling SubmitIntent");
    let response = usecases::submit_intent(
        &self.keypunk_service,
        &self.protocols,
        &intent,
        &derivation_path,
    )
    .await;

    match response {
        Ok(KeypunkdResponse::ArtifactPreview {
            raw_artifact,
            parsed_summary,
            signature,
            keypunkd_public_key,
        }) => PaypunkdResponse::SignablePreview {
            raw_artifact,
            parsed_summary,
            keypunkd_signature: signature,
            keypunkd_public_key,
        },
        Ok(KeypunkdResponse::ArtifactAuthorized { signed_artifact }) => {
            PaypunkdResponse::SignatureApproved { signed_artifact }
        }
        Ok(KeypunkdResponse::Error { message }) => PaypunkdResponse::Error { message },
        Err(e) => PaypunkdResponse::Error { message: e },
        _ => PaypunkdResponse::Error {
            message: "unexpected response from keypunkd".to_string(),
        },
    }
}
```

Note: The `PaypunkdResponse::SignablePreview` and `PaypunkdResponse::SignatureApproved`
variants already exist in `paypunkd/src/messages.rs`. No new variants needed.

### 3. `api/src/functions.rs`

Update `submit_intent` to return `Result<SubmitIntentResult, String>` instead of
`Result<(Vec<u8>, Vec<u8>, Vec<u8>, [u8; 32]), String>`:

```rust
pub async fn submit_intent(
    service: &paypunkd::services::PaypunkService,
    intent: Intent,
    derivation_path: &str,
) -> Result<SubmitIntentResult, String> {
    match service.submit_intent(intent, derivation_path).await? {
        PaypunkdResponse::SignablePreview {
            raw_artifact,
            parsed_summary,
            keypunkd_signature,
            keypunkd_public_key,
        } => Ok(SubmitIntentResult::SignablePreview {
            raw_artifact,
            parsed_summary,
            keypunkd_signature,
            keypunkd_public_key,
        }),
        PaypunkdResponse::SignatureApproved { signed_artifact } => {
            Ok(SubmitIntentResult::SignatureApproved { signed_artifact })
        }
        PaypunkdResponse::Error { message } => Err(message),
        _ => Err("unexpected response".to_string()),
    }
}
```

Add imports:
```rust
use paypunk_types::{SubmitIntentResult, PaypunkdResponse};
```

### 4. `api/src/client.rs`

Update the `submit_intent` method return type to `Result<SubmitIntentResult, String>`:

```rust
pub async fn submit_intent(
    &self,
    intent: Intent,
    derivation_path: &str,
) -> Result<SubmitIntentResult, String> {
    functions::submit_intent(&self.service, intent, derivation_path).await
}
```

### 5. `tui/src/api/real.rs`

Update `submit_send_review` (around line 356) where it calls
`self.client.submit_intent(...)`. The return type is now `SubmitIntentResult`
instead of a 4-tuple. Update the match:

```rust
match self.client.submit_intent(intent, &derivation_path).await {
    Ok(SubmitIntentResult::SignablePreview {
        raw_artifact,
        parsed_summary,
        keypunkd_signature,
        keypunkd_public_key,
    }) => {
        let pending = PendingSend {
            raw_artifact,
            keypunkd_signature,
            keypunkd_public_key,
            derivation_path,
            protocol,
        };
        *self.pending.lock().unwrap() = Some(pending);
        // ... parse summary and return SendReviewData ...
    }
    Ok(SubmitIntentResult::SignatureApproved { .. }) => {
        // Should not happen in keypunkd mode; return error SendReviewData
        SendReviewData { /* error fallback */ }
    }
    Err(e) => SendReviewData { /* error fallback */ }
}
```

### 6. `cli/src/main.rs`

Update `submit_intent_flow` (around line 702) where it calls `client.submit_intent(...)`.
The return type is now `SubmitIntentResult`:

```rust
match client.submit_intent(intent, &derivation_path).await? {
    SubmitIntentResult::SignablePreview {
        raw_artifact,
        parsed_summary,
        keypunkd_signature,
        keypunkd_public_key,
    } => {
        // existing flow: save to pending.intent, print instructions
        // ...
    }
    SubmitIntentResult::SignatureApproved { signed_artifact } => {
        // signer mode: should not happen in CLI flow
        println!("Transaction signed by offline signer");
        println!("Signed artifact: {} bytes", signed_artifact.len());
    }
}
```

### 7. `tests/tests/integration_test.rs`

Update `test_eth_send_full_flow` (around line 354) where it calls
`client.submit_intent(...)`:

```rust
let result = client
    .submit_intent(intent, path)
    .await
    .expect("submit_intent should succeed");

match result {
    SubmitIntentResult::SignablePreview {
        raw_artifact,
        parsed_summary,
        keypunkd_signature,
        keypunkd_public_key,
    } => {
        let summary: ArtifactSummary =
            postcard::from_bytes(&parsed_summary).expect("should deserialize");
        match &summary {
            ArtifactSummary::Ethereum(eth) => {
                assert_eq!(eth.to, "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045");
            }
            _ => panic!("expected Ethereum summary"),
        }
        // ... rest of test ...
    }
    _ => panic!("expected SignablePreview"),
}
```

## Acceptance criteria

1. `cargo build --workspace` succeeds.
2. `cargo test --workspace` passes.
3. `cargo test -p tests` passes — `test_eth_send_full_flow` uses `SubmitIntentResult`.
4. `cargo fmt --all` produces no changes.
5. `paypunkd::usecases::submit_intent` returns `Result<KeypunkdResponse, String>`.
6. `api::functions::submit_intent` returns `Result<SubmitIntentResult, String>`.
7. `api::Client::submit_intent` returns `Result<SubmitIntentResult, String>`.
8. The keypunkd mode flow is unchanged — `SubmitIntentResult::SignablePreview` is
   returned and consumed identically to the old 4-tuple.
9. `SubmitIntentResult::SignatureApproved` is handled gracefully (even though it
   won't occur until the signer is connected).

## Context

This step is plumbing — the keypunkd mode flow is functionally identical. The only
difference is that `submit_intent` now returns a typed enum instead of an opaque
4-tuple. This enables the caller (TUI, CLI) to branch on `SignablePreview` vs
`SignatureApproved`.

In keypunkd mode, the `preview_artifact` call always returns
`KeypunkdResponse::ArtifactPreview`, which maps to `PaypunkdResponse::SignablePreview`,
which maps to `SubmitIntentResult::SignablePreview`. The `SignatureApproved` path
is only taken when the signer app directly returns a signed artifact (via the
bridge), which won't happen until the CLI config switch and signer app are wired.

## Verification

```bash
cargo build --workspace
cargo test --workspace
cargo test -p tests
cargo fmt --all
```

After verification, move this file to `./project/done/06_step.md` and commit with:

```
git add -A && git commit -m "paypunkd,api: return SubmitIntentResult enum from submit_intent"
```