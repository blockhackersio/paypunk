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

A config flag (CLI `--signer` or env var) determines which daemon runs. When set:
- `keypunkd` is not spawned
- The bridge is spawned instead
- paypunkd connects to the bridge socket
- The TUI adapts: skips preview display, skips password prompt, shows "Awaiting signer..."

### Signer mode flow

```
TUI → api::Client::submit_intent(intent, path)
  → paypunkd → protocol.build() → unsigned PCZT
  → paypunkd → [bridge socket] → KeypunkdRequest::PreviewArtifact { raw_artifact, protocol, chain_id, derivation_path }
  → bridge → QR code
  → signer scans QR → deserializes KeypunkdRequest
  → signer: ZcashSignerProtocol::parse_artifact() → displays preview
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
  → keypunkd preview → ArtifactPreview → paypunkd
  → paypunkd returns SignablePreview → TUI shows preview
TUI → api::Client::approve_signature(raw, sig, password, path)
  → paypunkd → KeypunkdRequest::AuthorizeArtifact → keypunkd
  → keypunkd verifies commitment → decrypts seed → signs → ArtifactAuthorized → paypunkd
  → paypunkd returns SignatureApproved → TUI
TUI → broadcast_transaction
```

---

## Changes by Crate

### 1. `types/src/lib.rs` — Add `KeypunkdRequest`, `KeypunkdResponse`, `SubmitIntentResult`

Move `KeypunkdRequest` and `KeypunkdResponse` from `keypunkd/src/messages.rs`. Add `chain_id: ChainId` to `PreviewArtifact` variant.

```rust
#[derive(Debug, Serialize, Deserialize)]
pub enum KeypunkdRequest {
    GetEncryptionKey,
    GenerateSeed { encrypted_password: Vec<u8>, client_public_key: [u8; 32] },
    RestoreSeed { encrypted_mnemonic: Vec<u8>, encrypted_password: Vec<u8>, client_public_key: [u8; 32] },
    PreviewArtifact {
        raw_artifact: Vec<u8>,
        protocol: ProtocolId,
        chain_id: ChainId,          // NEW: CAIP-2 chain identifier
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

/// Result of submit_intent — adapts to the backend mode.
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
    // chain(), export_viewing(), parse_artifact(), sign()
    // sign_transaction_inner() — moved from ZcashProtocol
    // Uses crate::common::{ZCASH_COIN_TYPE, account_from_path}
    // KeyRef enum moves here
}
```

Dependencies only: `pczt`, `orchard`, `zcash_primitives`, `zcash_keys`, `zcash_protocol`, `zcash_proofs`, `zip32`, `zcash_address`, `zcash_transparent`, `postcard`, `paypunk-types`, `async-trait`, `rand`, `rand_core`, `hex`, `thiserror`, `tracing`.

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

### 9. `paypunkd/src/services.rs` — Update `preview_artifact` signature

Add `chain_id` parameter, return `KeypunkdResponse` instead of destructured fields:

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

```rust
pub async fn submit_intent(...) -> PaypunkdResponse {
    let chain_id = protocol.chain_id();
    let response = keypunk_service.preview_artifact(raw_artifact, protocol_id, chain_id, derivation_path).await?;

    match response {
        KeypunkdResponse::ArtifactPreview { raw_artifact, parsed_summary, signature, keypunkd_public_key } => {
            PaypunkdResponse::SignablePreview { raw_artifact, parsed_summary, keypunkd_signature: signature, keypunkd_public_key }
        }
        KeypunkdResponse::ArtifactAuthorized { signed_artifact } => {
            PaypunkdResponse::SignatureApproved { signed_artifact }
        }
        KeypunkdResponse::Error { message } => {
            PaypunkdResponse::Error { message }
        }
        _ => PaypunkdResponse::Error { message: "unexpected response".to_string() }
    }
}
```

### 11. `paypunkd/src/paypunkd.rs` — `submit_intent` handler

Returns `PaypunkdResponse::SignatureApproved` when the usecase detects the signer response. The `SignablePreview` path is unchanged for keypunkd mode.

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

```rust
async fn submit_send_review(&self, input: SendReviewInput) -> SendReviewData {
    // ... build intent ...
    match self.client.submit_intent(intent, &derivation_path).await {
        Ok(SubmitIntentResult::SignatureApproved { signed_artifact }) => {
            // Signer mode: store signed artifact, return marker to skip review
            *self.signed.lock().unwrap() = Some((protocol, signed_artifact));
            SendReviewData { skip_review: true, ... }
        }
        Ok(SubmitIntentResult::SignablePreview { raw_artifact, parsed_summary, keypunkd_signature, keypunkd_public_key }) => {
            // Keypunkd mode: existing flow
            // ... store PendingSend, parse ArtifactSummary, return SendReviewData ...
        }
        Err(e) => SendReviewData { error: e, ... },
    }
}
```

**`tui/src/screens/send.rs`:**

When `SendReviewData.skip_review` is true:
- Skip `SendStep::Review` entirely
- Jump to `SendStep::Sending` with "Awaiting signer approval..." spinner
- `submit_send_confirm` calls `broadcast_transaction` directly (no password, no `approve_signature`)

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
    Previewing { raw_artifact: Vec<u8>, parsed_summary: Vec<u8>, derivation_path: String },
    Signing,
    Error(String),
}

impl SignerState {
    pub fn create() -> Self { /* hardcoded test phrase, BIP39 seed */ }

    pub fn handle_request(&mut self, request: &KeypunkdRequest) -> KeypunkdResponse {
        match request {
            KeypunkdRequest::PreviewArtifact { raw_artifact, protocol, chain_id, derivation_path } => {
                // Lazy-init protocol from chain_id
                // Parse artifact: protocol.parse_artifact(raw_artifact)
                // Store preview state, return ArtifactPreview for UI
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
| `PreviewPage` | `/preview` | Shows ArtifactSummary (to, amount, fee, memo). Approve/Reject. |
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
| 1 | Move `KeypunkdRequest`/`KeypunkdResponse` to `types`, add `chain_id`, `SubmitIntentResult` | `types`, `keypunkd` |
| 2 | Create `protocols/zcash/src/common.rs` with shared helpers | `protocols/zcash` |
| 3 | Feature-gate wallet deps in `protocols/zcash/Cargo.toml` | `protocols/zcash` |
| 4 | Create `protocols/zcash/src/signer.rs` with `ZcashSignerProtocol` | `protocols/zcash` |
| 5 | Remove `SignerProtocol` impl from `ZcashProtocol` in `protocol.rs` | `protocols/zcash` |
| 6 | Update `protocols/zcash/src/lib.rs` exports and `#[cfg]` gates | `protocols/zcash` |
| 7 | Update `keypunkd/src/run.rs` to use `ZcashSignerProtocol` | `keypunkd` |
| 8 | Update `paypunkd/src/services.rs` and `usecases.rs` for signer-aware flow | `paypunkd` |
| 9 | Update `paypunkd/src/paypunkd.rs` handler | `paypunkd` |
| 10 | Update `api/src/functions.rs` and `client.rs` with `SubmitIntentResult` | `api` |
| 11 | Update `tui/src/api/real.rs` for `Signed` variant | `tui` |
| 12 | Update `tui/src/screens/send.rs` for signer mode flow | `tui` |
| 13 | Update `tests/` | `tests` |
| 14 | Signer: `Cargo.toml` deps + `signer.rs` module + commands | `signer/src-tauri` |
| 15 | Signer: React pages | `signer/src` |
| 16 | Config switch (CLI flag) | `cli`, `config` |
| 17 | Build and verify | all |

---

## Non-changes

- `bridge/` — no changes
- `keypunkd/src/keypunk.rs` — no changes
- `keypunkd/src/usecases.rs` — no changes
- `keypunkd/src/crypto.rs` — no changes
- `keypunkd/src/key.rs` — no changes
- `keypunkd/src/protocol.rs` — no changes
- `ipc/` — no changes
- `pong/` — no changes
- `ping/` — no changes
- `protocols/ethereum/` — no changes

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
9. **TUI async blocking**: `submit_send_review` blocks for 30-60s in signer mode (Orchard proving). The TUI must not block the event loop. Solution: `WalletApi` trait becomes `Send`, `handle_input` spawns a `tokio::spawn` background task and returns immediately, `tick()` polls for completion. The TUI shows "Awaiting signer..." spinner during the wait.

---

10. **Ethereum support in phase 1**: The BIP39 test phrase derives keys for any chain. The signer holds a `ProtocolService` (same pattern as keypunkd) with both `ZcashSignerProtocol` and `EthereumProtocol` registered. The `chain_id` in `PreviewArtifact` dispatches to the correct protocol. Both chains supported from day one.

---

## Open Decisions

1. **Bridge timeout**: No timeout. The bridge waits indefinitely for the signer to respond. The user is expected to complete signing on the phone.

2. **Seed phrase for phase 1**: The hardcoded test phrase is a bootstrap convenience. The signer is network-agnostic — the `chain_id` from the message drives everything. When the seed changes (e.g., randomly generated), it works on any network (mainnet, testnet, regtest).

3. **Ethereum**: Not a concern for this plan. Focus on the Zcash path.