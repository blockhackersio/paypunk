# Offline Signer — Implementation Plan

## Overview

Refactor the signing architecture to support an offline signer app (Tauri mobile) that holds the seed, displays transaction previews, and signs via QR code. The existing two-daemon keypunkd mode remains available. Both modes coexist — the TUI adapts to whichever response it receives.

---

## Architecture

### Two modes

| Mode | Daemons | Signing flow |
|------|---------|-------------|
| **Keypunkd mode** (existing) | paypunkd + keypunkd | Two-phase: preview in TUI → password → sign |
| **Signer mode** (new) | paypunkd + bridge + signer app | Single-phase: QR → signer shows preview → user approves → sign → signed artifact returned |

### Config switch

A config flag set in the config file `offline_signer:true` `--offline-signer` (`PaypunkConfig`) determines which daemon runs. When set:
- `keypunkd` is not spawned
- The bridge is spawned instead
- paypunkd connects to the bridge socket
- the tui passes the config flag as it is being constructed.
- The TUI adapts: skips preview display, skips password prompt, shows "Awaiting signer..."

### Signer mode flow

```
TUI → api::Client::submit_intent(intent, path)
  → paypunkd → protocol.build() → unsigned PCZT
  → paypunkd → [bridge socket] → KeypunkdRequest::PreviewArtifact { raw_artifact, protocol, chain_id, derivation_path }
  → bridge → QR code
  → signer scans QR → deserializes KeypunkdRequest
  → signer: ZcashSignerProtocol::parse_artifact() → real ArtifactSummary (recipient, amount, fee)
  → signer displays preview
  → user approves
  → signer: ZcashSignerProtocol::sign(seed, path, raw_artifact) [real Orchard proving + signing]
  → signer displays response QR: KeypunkdResponse::ArtifactAuthorized { signed_artifact }
  → bridge camera scans response QR → POST /response → bridge → paypunkd
  → paypunkd returns PaypunkdResponse::SignatureApproved { signed_artifact }
  → TUI: broadcast_transaction
```

Single bridge message, single QR scan. No `GetEncryptionKey`, no `AuthorizeArtifact`, no `ArtifactPreview` roundtrip through the bridge.

### Keypunkd mode flow (unchanged)

```
TUI → api::Client::submit_intent(intent, path)
  → paypunkd → protocol.build() → unsigned PCZT
  → paypunkd → [keypunkd socket] → KeypunkdRequest::PreviewArtifact → keypunkd
  → keypunkd: parse_artifact() → real ArtifactSummary → signs commit hash
  → ArtifactPreview → paypunkd
  → paypunkd returns SignablePreview → TUI shows preview
TUI → api::Client::approve_signature(raw, sig, password, path)
  → paypunkd → KeypunkdRequest::AuthorizeArtifact → keypunkd
  → keypunkd re-parses artifact → verifies commit hash → decrypts seed → signs → ArtifactAuthorized → paypunkd
  → paypunkd returns SignatureApproved → TUI
TUI → broadcast_transaction
```

---

## Changes by Crate

### 1. `types/src/lib.rs` — Add `KeypunkdRequest`, `KeypunkdResponse`, `SubmitIntentResult`, update `SignerProtocol`

Move `KeypunkdRequest` and `KeypunkdResponse` from `keypunkd/src/messages.rs`. Add `chain_id` to `PreviewArtifact`. Remove `chain()` from `SignerProtocol`.

```rust
#[derive(Debug, Serialize, Deserialize)]
pub enum KeypunkdRequest {
    GetEncryptionKey,
    GenerateSeed { encrypted_password: Vec<u8>, client_public_key: [u8; 32] },
    RestoreSeed { encrypted_mnemonic: Vec<u8>, encrypted_password: Vec<u8>, client_public_key: [u8; 32] },
    PreviewArtifact {
        raw_artifact: Vec<u8>,
        protocol: ProtocolId,
        chain_id: ChainId,          // NEW: CAIP-2 chain identifier for signer auto-config
        derivation_path: String,
    },
    AuthorizeArtifact { encrypted_payload: Vec<u8>, ephemeral_public_key: [u8; 32], derivation_path: String },
    ExportViewingKey { encrypted_password: Vec<u8>, client_public_key: [u8; 32], protocol: ProtocolId, derivation_path: String },
    HasSeed,
    VerifyPassword { encrypted_password: Vec<u8>, client_public_key: [u8; 32] },
    BulkExportViewingKeys { encrypted_password: Vec<u8>, client_public_key: [u8; 32], paths: Vec<(ProtocolId, String)> },
    ExportMnemonic { encrypted_password: Vec<u8>, client_public_key: [u8; 32] },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum KeypunkdResponse {
    EncryptionKey { key: [u8; 32] },
    SeedGenerated { encrypted_mnemonic: Vec<u8> },
    SeedRestored,
    ArtifactPreview { raw_artifact: Vec<u8>, parsed_summary: Vec<u8>, signature: Vec<u8>, keypunkd_public_key: [u8; 32] },
    ArtifactAuthorized { signed_artifact: Vec<u8> },
    ViewingKey { key: Vec<u8> },
    HasSeed { exists: bool },
    PasswordVerified,
    ViewingKeys { keys: Vec<(ProtocolId, String, Vec<u8>)> },
    MnemonicExported { encrypted_mnemonic: Vec<u8> },
    Error { message: String },
}

/// A single output entry in the artifact summary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutputEntry {
    pub address: String,
    pub amount: String,
}

/// Protocol-specific artifact summary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ArtifactSummary {
    Zcash(ZcashArtifactSummary),
    Ethereum(EthereumArtifactSummary),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ZcashArtifactSummary {
    pub outputs: Vec<OutputEntry>,
    pub fee: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EthereumArtifactSummary {
    pub to: String,
    pub amount: String,
    pub fee: String,
    pub nonce: u64,
}

/// Result of submit_intent for the API/TUI layer.
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

/// A single output entry in the artifact summary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutputEntry {
    pub address: String,
    pub amount: String,
}

/// Signer-side protocol operations: export viewing keys, parse unsigned
/// artifacts for preview, and sign artifacts.
#[async_trait::async_trait]
pub trait SignerProtocol: Send + Sync {
    fn export_viewing(&self, seed: &[u8; 64], path: &str) -> Result<Vec<u8>, String>;
    fn parse_artifact(&self, artifact: &[u8]) -> Result<Vec<u8>, String>;
    fn sign(&self, seed: &[u8; 64], path: &str, artifact: &[u8]) -> Result<Vec<u8>, String>;
}
```

### 2. `keypunkd/src/messages.rs` — Re-export

Replace entire file with:

```rust
pub use paypunk_types::{KeypunkdRequest, KeypunkdResponse};
```

### 3. `protocols/zcash/Cargo.toml` — Feature-gate wallet deps

```toml
[features]
default = ["wallet"]
wallet = [
    "dep:tactix",
    "dep:tokio",
    "dep:zcash_client_backend",
    "dep:zcash_client_sqlite",
    "dep:rusqlite",
    "dep:tonic",
    "dep:reqwest",
    "dep:serde_json",
    "dep:secrecy",
]
```

Make these dependencies `optional = true`. `tracing` and `thiserror` remain always available.

### 4. `protocols/zcash/src/common.rs` — New file, shared helpers

```rust
use orchard::Address;
use zcash_address::unified;
use zcash_address::{ToAddress, ZcashAddress};
use zcash_protocol::consensus::NetworkType;

pub const ZCASH_COIN_TYPE: u32 = 133;

pub fn account_from_path(path: &str) -> Result<u32, String> {
    let account_str = path
        .rsplit('\'')
        .nth(1)
        .and_then(|s| s.split('/').last())
        .ok_or_else(|| format!("invalid derivation path: {path}"))?;
    account_str
        .parse()
        .map_err(|_| format!("invalid account index in path: {path}"))
}

/// Decode a raw Orchard address ([u8; 43]) to a human-readable unified address.
pub fn decode_orchard_recipient(raw: &[u8; 43], net: NetworkType) -> Option<String> {
    let orchard_addr = Address::from_raw_address_bytes(raw).into_option()?;
    let raw = orchard_addr.to_raw_address_bytes();
    let ua = unified::Address::try_from_items(vec![unified::Receiver::Orchard(raw)]).ok()?;
    let zaddr = ZcashAddress::from_unified(net, ua);
    Some(zaddr.encode())
}
```

Both `protocol.rs` and `signer.rs` use `crate::common::*`.

### 5. `protocols/zcash/src/signer.rs` — New file, `ZcashSignerProtocol`

```rust
pub struct ZcashSignerProtocol {
    pub params: LocalNetwork,
    network_type: NetworkType,
}

impl ZcashSignerProtocol {
    pub fn new(params: LocalNetwork, network_type: NetworkType) -> Self {
        Self { params, network_type }
    }
}

#[async_trait]
impl SignerProtocol for ZcashSignerProtocol {
    fn export_viewing(&self, seed: &[u8; 64], path: &str) -> Result<Vec<u8>, String> {
        // Same as current ZcashProtocol::export_viewing
    }

    fn parse_artifact(&self, artifact: &[u8]) -> Result<Vec<u8>, String> {
        let pczt = pczt::Pczt::parse(artifact).map_err(|e| format!("PCZT parse failed: {e:?}"))?;

        // Extract fee from Orchard value sum
        let (value_sum, negative) = pczt.orchard().value_sum();
        let fee = if *negative { 0u64 } else { *value_sum };

        // Extract all outputs (recipient + change) from Orchard actions
        let mut outputs = Vec::new();
        for action in pczt.orchard().actions() {
            if let (Some(recipient_raw), Some(value)) =
                (action.output().recipient(), action.output().value())
            {
                if let Some(addr) = decode_orchard_recipient(recipient_raw, self.network_type) {
                    outputs.push(OutputEntry { address: addr, amount: value.to_string() });
                }
            }
        }

        let summary = ArtifactSummary::Zcash(ZcashArtifactSummary {
            outputs,
            fee: fee.to_string(),
        });

        postcard::to_allocvec(&summary).map_err(|e| format!("serialize summary failed: {e}"))
    }

    fn sign(&self, seed: &[u8; 64], path: &str, artifact: &[u8]) -> Result<Vec<u8>, String> {
        let account = account_from_path(path)?;
        self.sign_transaction_inner(seed, account, artifact)
    }
}

// sign_transaction_inner() — moved from ZcashProtocol
// KeyRef enum — moved from ZcashProtocol
// decode_orchard_recipient() — in crate::common
```

Dependencies only: `pczt`, `orchard`, `zcash_primitives`, `zcash_keys`, `zcash_protocol`, `zcash_proofs`, `zip32`, `zcash_address`, `zcash_transparent`, `postcard`, `paypunk-types`, `async-trait`, `rand`, `rand_core`, `hex`, `thiserror`, `tracing`.

`parse_artifact` extracts real data from the PCZT:
- **Recipient**: `action.output().recipient()` → `[u8; 43]` → `orchard::Address::from_raw_address_bytes` → `unified::Address` → `ZcashAddress::encode()`
- **Amount**: `action.output().value()` → `u64` (zatoshis)
- **Fee**: `pczt.orchard().value_sum()`

Both `recipient` and `value` are always set by the Constructor (`create_pczt_from_proposal`). The `ArtifactSummary` is self-contained in the PCZT — no external data needed. The commit hash in `authorize_artifact` re-parses to the same bytes.

### 6. `protocols/zcash/src/protocol.rs` — Remove `SignerProtocol` impl

Remove `impl SignerProtocol for ZcashProtocol` (lines 79-135). Remove `sign_transaction_inner` (lines 137-210). Remove `KeyRef` enum. Keep `impl Protocol for ZcashProtocol` unchanged. Import `COIN_TYPE` and `account_from_path` from `crate::common`.

### 7. `protocols/zcash/src/lib.rs` — Gate wallet code, export signer

```rust
pub mod address;
pub mod common;
pub mod protocol;
pub mod signer;

#[cfg(feature = "wallet")]
pub mod lsp_client;
#[cfg(feature = "wallet")]
pub mod scan_actor;
#[cfg(feature = "wallet")]
pub mod wallet_actor;

pub use protocol::ZcashProtocol;
pub use signer::ZcashSignerProtocol;

#[cfg(feature = "wallet")]
pub use scan_actor::{Sync, SyncNewAccount};
#[cfg(feature = "wallet")]
pub use wallet_actor::{...};

#[cfg(feature = "wallet")]
pub fn create_protocol(...) -> ZcashStack { ... }

#[cfg(feature = "wallet")]
pub struct ZcashStack { ... }

// Always available:
pub fn to_local_params(...) -> LocalNetwork { ... }
pub fn derivation_path(account: u32) -> String { ... }
```

### 8. `keypunkd/src/run.rs` — Use `ZcashSignerProtocol`

```rust
// Before:
Box::new(paypunk_chains_zcash::protocol::ZcashProtocol::new(
    to_local_params(params, network_type), network_type, None, None, None,
))

// After:
Box::new(paypunk_chains_zcash::signer::ZcashSignerProtocol::new(
    to_local_params(params, network_type), network_type,
))
```

### 9. `keypunkd/src/services.rs` — Update `preview_artifact` signature

Add `chain_id` parameter:

```rust
pub async fn preview_artifact(
    &self,
    raw_artifact: Vec<u8>,
    protocol: ProtocolId,
    chain_id: ChainId,
    derivation_path: String,
) -> Result<KeypunkdResponse, String>
```

### 10. `paypunkd/src/usecases.rs` — Signer-aware `submit_intent`

Return `Result<KeypunkdResponse, String>` directly. The `submit_intent` handler maps to `PaypunkdResponse`:

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

    keypunk_service.preview_artifact(
        raw_artifact,
        protocol_id,
        chain_id,
        derivation_path.to_string(),
    ).await
}
```

### 11. `paypunkd/src/paypunkd.rs` — `submit_intent` handler

Match on `KeypunkdResponse` to return the appropriate `PaypunkdResponse`:

```rust
async fn submit_intent(&self, intent: Intent, derivation_path: String) -> PaypunkdResponse {
    let response = usecases::submit_intent(
        &self.keypunk_service, &self.protocols, &intent, &derivation_path,
    ).await;
    match response {
        Ok(KeypunkdResponse::ArtifactPreview { raw_artifact, parsed_summary, signature, keypunkd_public_key }) => {
            PaypunkdResponse::SignablePreview { raw_artifact, parsed_summary, keypunkd_signature: signature, keypunkd_public_key }
        }
        Ok(KeypunkdResponse::ArtifactAuthorized { signed_artifact }) => {
            PaypunkdResponse::SignatureApproved { signed_artifact }
        }
        Ok(KeypunkdResponse::Error { message }) => PaypunkdResponse::Error { message },
        Err(e) => PaypunkdResponse::Error { message: e },
        _ => PaypunkdResponse::Error { message: "unexpected response".to_string() },
    }
}
```

### 12. `api/src/functions.rs` — `SubmitIntentResult` enum

```rust
pub async fn submit_intent(service, intent, derivation_path) -> Result<SubmitIntentResult, String> {
    match service.submit_intent(intent, derivation_path).await? {
        PaypunkdResponse::SignablePreview { raw_artifact, parsed_summary, keypunkd_signature, keypunkd_public_key } => {
            Ok(SubmitIntentResult::SignablePreview { raw_artifact, parsed_summary, keypunkd_signature, keypunkd_public_key })
        }
        PaypunkdResponse::SignatureApproved { signed_artifact } => {
            Ok(SubmitIntentResult::SignatureApproved { signed_artifact })
        }
        PaypunkdResponse::Error { message } => Err(message),
        _ => Err("unexpected response".to_string()),
    }
}
```

### 13. `api/src/client.rs` — Update return type

```rust
pub async fn submit_intent(&self, intent: Intent, derivation_path: &str)
    -> Result<SubmitIntentResult, String>
```

### 14. TUI changes

**`tui/src/api/real.rs` — `submit_send_review`:**

In signer mode, `submit_send_review` spawns a `tokio::spawn` background task that calls `submit_intent` (blocks 30-60s until signer responds), then `broadcast_transaction`. The result is sent through a `tokio::sync::oneshot` channel. The receiver is stored on `RealWalletApi` and polled in `tick()`.

`WalletApi` trait changes from `#[async_trait(?Send)]` to `#[async_trait]` (Send). `RealWalletApi` fields are already all `Send`.

```rust
// New field on RealWalletApi:
pending_send_result: Mutex<Option<oneshot::Receiver<Result<SendResult, String>>>>,

async fn submit_send_review(&self, input: SendReviewInput) -> SendReviewData {
    // ... build intent, get protocol, derivation_path ...
    if self.signer_mode {
        let (tx, rx) = oneshot::channel();
        *self.pending_send_result.lock().unwrap() = Some(rx);
        let client = /* clone client */;
        let protocol = /* protocol id */;
        tokio::spawn(async move {
            match client.submit_intent(intent, &derivation_path).await {
                Ok(SubmitIntentResult::SignatureApproved { signed_artifact }) => {
                    let result = client.broadcast_transaction(protocol, signed_artifact).await;
                    let _ = tx.send(Ok(SendResult { tx_hash: result.unwrap_or_default(), ... }));
                }
                Ok(_) => { let _ = tx.send(Err("unexpected preview in signer mode".into())); }
                Err(e) => { let _ = tx.send(Err(e)); }
            }
        });
        SendReviewData { skip_review: true, ... }
    } else {
        // Keypunkd mode: existing flow unchanged
        match self.client.submit_intent(intent, &derivation_path).await {
            Ok(SubmitIntentResult::SignablePreview { raw_artifact, parsed_summary, keypunkd_signature, keypunkd_public_key }) => {
                // store PendingSend, parse ArtifactSummary, return SendReviewData
            }
            Err(e) => SendReviewData { ... },
        }
    }
}
```

**`tui/src/api/types.rs` — `SendReviewData`:**

```rust
pub struct SendReviewData {
    pub to_address: String,
    pub amount: String,
    pub fee_estimate: String,
    pub total_amount: String,
    pub chain_id: String,
    pub nonce: u64,
    pub skip_review: bool,  // NEW: signer mode skips review screen
}
```

**`tui/src/screens/send.rs`:**

When `SendReviewData.skip_review` is true:
- Skip `SendStep::Review`, jump to `SendStep::Sending` with "Awaiting signer..." spinner
- `tick()` polls the oneshot receiver via `api.poll_send_result()`
- When complete, `self.result = Some(result)` and `self.step = SendStep::Confirm`
- No `submit_send_confirm` call — the background task already broadcasted

**`tui/src/lib.rs` — `run_tui` signature:**

```rust
pub async fn run_tui(socket_path: &str, shutdown: Option<Arc<AtomicBool>>, signer_mode: bool) -> io::Result<()>
```

**`cli/src/main.rs` — spawn logic:**

When `--signer` flag is set:
- Skip spawning `keypunkd`
- Spawn `bridge` instead
- Connect paypunkd to bridge socket
- Pass `signer_mode: true` to `run_tui()`

### 15. `signer/` — Signer app (Tauri)

**`signer/src-tauri/Cargo.toml`:**
```toml
paypunk-types = { path = "../../types" }
paypunk-chains-zcash = { path = "../../protocols/zcash", default-features = false }
bip39 = { workspace = true }
postcard = { workspace = true }
blake2 = { workspace = true }
```

**`signer/src-tauri/src/signer.rs` — Signing logic:**

```rust
pub struct SignerState {
    pub seed: [u8; 64],
    pub mnemonic: String,
    pub protocol: Option<ZcashSignerProtocol>,  // Lazy: constructed from chain_id in first message
    pub status: SignerStatus,
}

pub enum SignerStatus {
    Idle,
    Previewing { raw_artifact: Vec<u8>, summary: ArtifactSummary, derivation_path: String },
    Signing,
    Error(String),
}

impl SignerState {
    pub fn create() -> Self { /* hardcoded test phrase, BIP39 seed */ }

    pub fn handle_request(&mut self, request: &KeypunkdRequest) -> KeypunkdResponse {
        match request {
            KeypunkdRequest::PreviewArtifact { raw_artifact, protocol, chain_id, derivation_path } => {
                // Lazy-init protocol from chain_id
                let parsed = self.protocol.as_ref().unwrap().parse_artifact(raw_artifact)?;
                let summary: ArtifactSummary = postcard::from_bytes(&parsed)?;
                // Store preview state with real ArtifactSummary from PCZT
                self.status = SignerStatus::Previewing {
                    raw_artifact: raw_artifact.clone(),
                    summary,
                    derivation_path: derivation_path.clone(),
                };
                KeypunkdResponse::ArtifactPreview { ... }
            }
            _ => KeypunkdResponse::Error { message: "unsupported".into() }
        }
    }

    pub fn sign(&self) -> Result<Vec<u8>, String> {
        // protocol.sign(&self.seed, &path, &raw_artifact)
        // Real Orchard proving + signing
    }
}
```

**`signer/src-tauri/src/lib.rs` — Commands:**

```rust
const SIGNER_MODE: SignerMode = SignerMode::Signer;
enum SignerMode { Pong, Signer }

// generate_seed, get_signer_status, process_scanned_qr, approve_and_sign
```

**`signer/src/` — React pages:**

| Page | Route | Purpose |
|------|-------|---------|
| `OnboardingPage` | `/` | "Generate Seed" button |
| `ScanPage` | `/scan` | Barcode scanner, captures QR |
| `PreviewPage` | `/preview` | Shows outputs (address + amount) and fee. Approve/Reject. |
| `SigningPage` | `/signing` | Spinner during Orchard proving |
| `ResultPage` | `/result` | Shows response QR for bridge to scan |

### 16. `bridge/` — No changes

Format-agnostic relay. Receives IPC frames, displays as QR, scans response QR, POSTs to `/response`.

### 17. `tests/` — Update

- `integration_test.rs`: Use `ZcashSignerProtocol` for keypunkd protocol registration.
- `pczt_test.rs`: Use `ZcashSignerProtocol` for signing operations.

---

## Implementation Order

| Step | What | Crates |
|------|------|--------|
| 1 | Move `KeypunkdRequest`/`KeypunkdResponse` to `types`, add `chain_id`, `SubmitIntentResult`, remove `chain()` from `SignerProtocol` | `types`, `keypunkd`, `protocols/zcash`, `protocols/ethereum` |
| 2 | Create `protocols/zcash/src/common.rs` with shared helpers | `protocols/zcash` |
| 3 | Feature-gate wallet deps in `protocols/zcash/Cargo.toml` | `protocols/zcash` |
| 4 | Create `protocols/zcash/src/signer.rs` with `ZcashSignerProtocol` (real parse_artifact, sign_transaction_inner, KeyRef) | `protocols/zcash` |
| 5 | Remove `SignerProtocol` impl from `ZcashProtocol` in `protocol.rs` | `protocols/zcash` |
| 6 | Update `protocols/zcash/src/lib.rs` exports and `#[cfg]` gates | `protocols/zcash` |
| 7 | Update `keypunkd/src/run.rs` to use `ZcashSignerProtocol`, update `keypunkd/src/services.rs` `preview_artifact` signature | `keypunkd` |
| 8 | Update `paypunkd/src/usecases.rs` and `paypunkd/src/paypunkd.rs` for signer-aware flow | `paypunkd` |
| 9 | Update `api/src/functions.rs` and `client.rs` with `SubmitIntentResult` | `api` |
| 10 | Update `tui/src/api/real.rs` for signer mode (oneshot, tokio::spawn) | `tui` |
| 11 | Update `tui/src/screens/send.rs` for signer mode flow (skip_review, spinner) | `tui` |
| 12 | Update `tests/` | `tests` |
| 13 | Signer: `Cargo.toml` deps + `signer.rs` module + commands | `signer/src-tauri` |
| 14 | Signer: React pages | `signer/src` |
| 15 | Config switch (CLI flag) | `cli`, `config` |
| 16 | Build and verify | all |

---

## Non-changes

- `bridge/` — no changes
- `keypunkd/src/keypunk.rs` — no changes (PreviewArtifact gets new destructure fields but ignores them)
- `keypunkd/src/usecases.rs` — no changes
- `keypunkd/src/crypto.rs` — no changes
- `keypunkd/src/key.rs` — no changes
- `keypunkd/src/protocol.rs` — no changes
- `ipc/` — no changes
- `pong/` — no changes
- `ping/` — no changes
- `protocols/ethereum/` — no changes (except removing `chain()` from `SignerProtocol` impl)
- `AuthorizeArtifact` message — no changes (re-parses to same real ArtifactSummary)
- `SignablePreview` response — no changes (parsed_summary is now real data)
- `api/src/functions.rs::approve_signature` — no changes

---

## Resolved Decisions

1. **Network**: `chain_id: ChainId` added to `PreviewArtifact`. Signer auto-configures `ZcashSignerProtocol` from it. No network setting needed in signer.

2. **Shared helpers**: `account_from_path` and `ZCASH_COIN_TYPE` in `protocols/zcash/src/common.rs`, imported by both `protocol.rs` and `signer.rs`.

3. **tracing/thiserror**: Always available (not gated).

4. **No mock signing**: Real Orchard proving and signing on the signer device.

5. **Single-phase signer**: One QR scan, one bridge message. No `GetEncryptionKey`, no `AuthorizeArtifact` roundtrip.

6. **TUI adaptation**: `SubmitIntentResult` enum — `SignablePreview` (keypunkd mode, show review + password) or `SignatureApproved` (signer mode, skip review, show spinner, broadcast).

7. **Bridge**: No changes. Format-agnostic relay.

8. **Config switch**: `--signer` CLI flag on `paypunk` binary. When set: spawns bridge instead of keypunkd, connects paypunkd to bridge socket, passes `signer_mode: true` to `run_tui()`. The TUI knows upfront (not dynamically from response).

9. **TUI async blocking**: `submit_send_review` blocks for 30-60s in signer mode. Solution: `WalletApi` trait becomes `Send` (remove `?Send`), `submit_send_review` in signer mode spawns a `tokio::spawn` background task that calls `submit_intent` + `broadcast_transaction`, sends result via `tokio::sync::oneshot` channel. `SendScreen::tick()` polls the receiver via `try_recv()`. The TUI shows "Awaiting signer..." spinner during the wait.

10. **Ethereum support in phase 1**: The BIP39 test phrase derives keys for any chain. The signer holds a `ProtocolService` (same pattern as keypunkd) with both `ZcashSignerProtocol` and `EthereumProtocol` registered. The `chain_id` in `PreviewArtifact` dispatches to the correct protocol. Both chains supported from day one.

11. **Remove `chain()` from `SignerProtocol`**: `chain()` is dead code — only called in a test (`protocols/ethereum/src/protocol.rs:395`), never in production. The `chain_id` field in `PreviewArtifact` provides the same information. Remove `async fn chain(&self) -> ChainId` from the trait, remove the `impl`s in both `ZcashProtocol` and `EthereumProtocol`, and update the Ethereum test that calls it.

12. **Real `parse_artifact`**: `parse_artifact` extracts real data from the PCZT. For each Orchard action, the output's `recipient()` (raw `[u8; 43]`) is decoded to a human-readable address via `orchard::Address::from_raw_address_bytes` → `unified::Address` → `ZcashAddress::encode()`. The output's `value()` provides the amount in zatoshis. Fee is from `pczt.orchard().value_sum()`. Both `recipient` and `value` are always set by the Constructor. All outputs are listed in the `ArtifactSummary` — the user can verify the recipient is among them.

13. **`ArtifactSummary` as protocol-specific enum**: `ArtifactSummary` becomes an enum with per-protocol variants. Zcash is UTXO-based with multiple outputs (recipient + change). Ethereum is account-based with a single `to`/`amount`.

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ArtifactSummary {
    Zcash(ZcashArtifactSummary),
    Ethereum(EthereumArtifactSummary),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ZcashArtifactSummary {
    pub outputs: Vec<OutputEntry>,
    pub fee: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EthereumArtifactSummary {
    pub to: String,
    pub amount: String,
    pub fee: String,
    pub nonce: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutputEntry {
    pub address: String,
    pub amount: String,
}
```

`nonce`, `memo`, `protocol` fields removed — they're either protocol-specific or not extractable.

13. **Memo-less preview**: Memo is encrypted inside the PCZT's `enc_ciphertext` and not extractable without the viewing key. The TUI already shows the memo on the send form. The signer preview shows outputs + fee only.

14. **`skip_review` on `SendReviewData`**: When signer mode is active, `submit_send_review` sets `skip_review: true`. The `SendScreen` checks this and jumps directly to `SendStep::Sending` with a spinner. No review screen, no password prompt.

15. **`signer_mode` flag through `run_tui`**: `run_tui(socket_path, shutdown, signer_mode: bool)`. The `RealWalletApi` stores `signer_mode: bool` and branches in `submit_send_review`. The config switch (`--signer`) is handled in `cli/src/main.rs` which spawns the bridge instead of keypunkd and passes the flag.

16. **Seed separation**: Phase 1 is a demo with a hardcoded test phrase in the signer app. No assumption of seed parity between keypunkd mode and signer mode. The signer has a button to create a test account from the fixed phrase. Later, seed setup can be done through the mobile app.

---

## Open Decisions

1. **Bridge timeout**: No timeout. The bridge waits indefinitely for the signer to respond. The user is expected to complete signing on the phone.

2. **Seed phrase for phase 1**: The hardcoded test phrase is a bootstrap convenience. The signer is network-agnostic — the `chain_id` from the message drives everything. When the seed changes (e.g., randomly generated), it works on any network (mainnet, testnet, regtest).

