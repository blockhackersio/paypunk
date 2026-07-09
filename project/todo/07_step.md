# Step 7: TUI signer mode

## Goal

Add signer mode support to the TUI. When `signer_mode` is true: skip the review
screen, skip the password prompt, spawn a `tokio::spawn` background task for
`submit_intent` + `broadcast_transaction`, and poll for completion via a
`oneshot` channel. Add `skip_review` to `SendReviewData`. Change `WalletApi` trait
from `#[async_trait(?Send)]` to `#[async_trait]` (Send).

## Files to change

### 1. `tui/src/api/mod.rs` — WalletApi trait

Change `#[async_trait(?Send)]` to `#[async_trait]`:

```rust
#[async_trait]
pub trait WalletApi {
    // ... all methods unchanged ...
}
```

Also add `poll_send_result` to the trait:

```rust
async fn poll_send_result(&self) -> Option<SendResult>;
```

### 2. `tui/src/api/types.rs` — SendReviewData

Add `skip_review: bool` field:

```rust
#[derive(Debug, Clone)]
pub struct SendReviewData {
    pub to_address: String,
    pub amount: String,
    pub fee_estimate: String,
    pub total_amount: String,
    pub chain_id: String,
    pub nonce: u64,
    pub skip_review: bool,  // NEW
}
```

### 3. `tui/src/api/real.rs` — RealWalletApi

**Add fields** to `RealWalletApi`:

```rust
use tokio::sync::oneshot;

pub struct RealWalletApi {
    client: Client,
    pending: std::sync::Mutex<Option<PendingSend>>,
    pending_mnemonic: std::sync::Mutex<Option<Zeroizing<String>>>,
    protocol_metadata: std::sync::Mutex<HashMap<ProtocolId, ProtocolMetadata>>,
    signer_mode: bool,  // NEW
    pending_send_result: std::sync::Mutex<Option<oneshot::Receiver<SendResult>>>,  // NEW
}
```

**Update `connect`** to accept `signer_mode`:

```rust
pub async fn connect(socket_path: &str, signer_mode: bool) -> Result<Self, String> {
    let client = Client::connect(socket_path).await?;
    Ok(Self {
        client,
        pending: std::sync::Mutex::new(None),
        pending_mnemonic: std::sync::Mutex::new(None),
        protocol_metadata: std::sync::Mutex::new(HashMap::new()),
        signer_mode,
        pending_send_result: std::sync::Mutex::new(None),
    })
}
```

**Update `submit_send_review`** to branch on `signer_mode`:

```rust
async fn submit_send_review(&self, input: SendReviewInput) -> SendReviewData {
    // ... existing code to look up account, build intent, get derivation_path, protocol ...

    if self.signer_mode {
        let (tx, rx) = oneshot::channel();
        *self.pending_send_result.lock().unwrap() = Some(rx);
        let client = self.client.clone();  // Client must be Clone
        let protocol_id = protocol;  // ProtocolId
        let intent = intent;  // Intent
        let derivation_path = derivation_path;  // String

        tokio::spawn(async move {
            let result = match client.submit_intent(intent, &derivation_path).await {
                Ok(SubmitIntentResult::SignatureApproved { signed_artifact }) => {
                    match client.broadcast_transaction(protocol_id, signed_artifact).await {
                        Ok(tx_hash) => Ok(SendResult {
                            tx_hash,
                            status: "broadcasted".to_string(),
                            block_explorer_url: String::new(),
                        }),
                        Err(e) => Err(e),
                    }
                }
                Ok(_) => Err("unexpected preview in signer mode".to_string()),
                Err(e) => Err(e),
            };
            let _ = tx.send(result);
        });

        return SendReviewData {
            to_address: String::new(),
            amount: String::new(),
            fee_estimate: String::new(),
            total_amount: String::new(),
            chain_id: input.chain_id,
            nonce: 0,
            skip_review: true,
        };
    }

    // Keypunkd mode: existing flow unchanged
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
                            skip_review: false,
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
                            skip_review: false,
                        }
                    }
                }
            } else {
                SendReviewData { /* error fallback */ skip_review: false, .. }
            }
        }
        Ok(SubmitIntentResult::SignatureApproved { .. }) => {
            SendReviewData { /* shouldn't happen in keypunkd mode */ skip_review: false, .. }
        }
        Err(e) => SendReviewData { /* error fallback */ skip_review: false, .. }
    }
}
```

**Add `poll_send_result`**:

```rust
async fn poll_send_result(&self) -> Option<SendResult> {
    let mut guard = self.pending_send_result.lock().unwrap();
    if let Some(rx) = guard.as_mut() {
        match rx.try_recv() {
            Ok(Ok(result)) => {
                *guard = None;
                return Some(result);
            }
            Ok(Err(_e)) => {
                *guard = None;
                return Some(SendResult {
                    tx_hash: String::new(),
                    status: "failed".to_string(),
                    block_explorer_url: String::new(),
                });
            }
            Err(oneshot::error::TryRecvError::Empty) => {}
            Err(oneshot::error::TryRecvError::Closed) => {
                *guard = None;
            }
        }
    }
    None
}
```

**Check if `Client` is `Clone`**. If not, wrap it in `Arc`:

```rust
client: Arc<Client>,
```

If `Client` is already `Clone` (it wraps a `Recipient<IpcMessage>` which is Clone),
no change is needed.

### 4. `tui/src/api/mock.rs` — MockWalletApi

Update `submit_send_review` to return `SendReviewData` with `skip_review: false`:

```rust
async fn submit_send_review(&self, input: SendReviewInput) -> SendReviewData {
    // ... existing code ...
    SendReviewData {
        // ... existing fields ...
        skip_review: false,
    }
}
```

Add `poll_send_result` stub:

```rust
async fn poll_send_result(&self) -> Option<SendResult> {
    None
}
```

Change `#[async_trait(?Send)]` to `#[async_trait]` on the `impl WalletApi for MockWalletApi` block.

### 5. `tui/src/screens/send.rs`

**Handle `skip_review`** in the send flow. When the review data is received and
`skip_review` is true, skip the `SendStep::Review` step and jump directly to
`SendStep::Sending`:

Find where `submit_send_review` is called and the result is stored. After storing
`self.review_data = Some(data)`, add:

```rust
if data.skip_review {
    self.step = SendStep::Sending;
    self.spinner_frame = 0;
}
```

**In the `Sending` step**, poll for completion via `api.poll_send_result()`:

```rust
SendStep::Sending => {
    if self.review_data.as_ref().map(|d| d.skip_review).unwrap_or(false) {
        // Signer mode: poll the oneshot channel
        if let Some(result) = api.poll_send_result().await {
            self.result = Some(result);
            self.step = SendStep::Confirm;
        }
    } else {
        // Keypunkd mode: existing flow (broadcast via submit_send_confirm)
        // ... existing code ...
    }
}
```

**Note**: The `Sending` step in keypunkd mode calls `submit_send_confirm` which
does the broadcast. In signer mode, the background task already broadcasts, so
`submit_send_confirm` is NOT called.

### 6. `tui/src/lib.rs`

Update `run_tui` signature to accept `signer_mode`:

```rust
pub async fn run_tui(
    socket_path: &str,
    shutdown: Option<Arc<AtomicBool>>,
    signer_mode: bool,
) -> io::Result<()> {
    let api = RealWalletApi::connect(socket_path, signer_mode).await
        .map_err(|e| io::Error::new(io::ErrorKind::ConnectionRefused, e))?;
    // ... rest unchanged ...
}
```

### 7. `tui/Cargo.toml`

Add `tokio` dependency if not already present:

```toml
tokio = { workspace = true, features = ["sync"] }
```

The `sync` feature is needed for `tokio::sync::oneshot`.

## Acceptance criteria

1. `cargo build --workspace` succeeds.
2. `cargo test -p paypunk-tui` passes.
3. `cargo fmt --all` produces no changes.
4. `WalletApi` trait uses `#[async_trait]` (Send), not `#[async_trait(?Send)]`.
5. `SendReviewData` has `skip_review: bool`.
6. `RealWalletApi` has `signer_mode` and `pending_send_result` fields.
7. `run_tui` accepts `signer_mode: bool` parameter.
8. In keypunkd mode (signer_mode=false), the existing send flow is unchanged.
9. In signer mode (signer_mode=true), `submit_send_review`:
   - Spawns a `tokio::spawn` background task
   - Returns immediately with `skip_review: true`
   - The send screen skips `Review` and shows `Sending` spinner
   - `poll_send_result` retrieves the result when complete

## Context

The `WalletApi` trait change from `?Send` to `Send` is required for `tokio::spawn`
which requires the future to be `Send`. Both `MockWalletApi` and `RealWalletApi`
are already fully `Send` (all fields are `Send`), so this is safe.

In signer mode, the TUI does NOT call `submit_send_confirm` (no password prompt,
no approval). The background task handles the entire flow: `submit_intent` →
blocking wait for signer → `broadcast_transaction`. The TUI just shows a spinner
and polls for the result.

The `Client` must be `Clone` or wrapped in `Arc` for the `tokio::spawn` closure.
Check if `paypunk_api::Client` implements `Clone` — it wraps
`Recipient<IpcMessage>` which is `Clone`, so `Client` should be `Clone`. If not,
derive `Clone` on `Client` or wrap in `Arc`.

## Verification

```bash
cargo build --workspace
cargo test -p paypunk-tui
cargo test --workspace
cargo fmt --all
```

After verification, move this file to `./project/done/07_step.md` and commit with:

```
git add -A && git commit -m "tui: add signer mode with skip_review, oneshot polling, and Send trait"
```