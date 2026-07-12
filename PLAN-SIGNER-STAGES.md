# Offline Signer — Staged Implementation

Each stage compiles independently and existing tests pass. Every stage updates the
tests it touches so `cargo test --workspace` (or `cargo test -p <crate>`) stays green.

---

## Stage 1: Types crate — move enums, add new types, remove `chain()`

**Goal**: Move `KeypunkdRequest`/`KeypunkdResponse` into `types`, add `chain_id` to
`PreviewArtifact`, add `SubmitIntentResult`, change `ArtifactSummary` from struct to
enum, remove `chain()` from `SignerProtocol`. All existing code adapts mechanically.

**Files touched**:

| File | Change |
|------|--------|
| `types/src/lib.rs` | Add `OutputEntry`, `ZcashArtifactSummary`, `EthereumArtifactSummary`. Change `ArtifactSummary` from struct to enum. Add `KeypunkdRequest` (with `chain_id` on `PreviewArtifact`), `KeypunkdResponse`. Add `SubmitIntentResult`. Remove `async fn chain()` from `SignerProtocol`. |
| `keypunkd/src/messages.rs` | Replace with `pub use paypunk_types::{KeypunkdRequest, KeypunkdResponse};` |
| `keypunkd/src/keypunk.rs` | Update match arms on `PreviewArtifact` to destructure `chain_id` (ignore it). |
| `keypunkd/src/services.rs` | Add `chain_id: ChainId` parameter to `preview_artifact()`. |
| `protocols/zcash/src/protocol.rs` | Remove `async fn chain()` from `impl SignerProtocol`. Update `parse_artifact` to return `ArtifactSummary::Zcash(...)`. Update `export_viewing`/`sign` to import `SignerProtocol` from `paypunk_types` (no change to logic). |
| `protocols/ethereum/src/protocol.rs` | Remove `async fn chain()` from `impl SignerProtocol`. Update `parse_artifact` to return `ArtifactSummary::Ethereum(...)`. Update test `test_chain_id` to exercise `Protocol::chain_id()` instead of `SignerProtocol::chain()`. |
| `paypunkd/src/usecases.rs` | Pass `chain_id` to `preview_artifact`. Get `chain_id` from `protocol.chain_id()` (the `Protocol` trait method, not the removed `SignerProtocol` one). Update imports to use `paypunk_types::KeypunkdResponse`. |
| `tui/src/api/real.rs` | Update deserialization of `parsed_summary` — match on `ArtifactSummary::Zcash(...)` and `ArtifactSummary::Ethereum(...)`. |
| `tests/tests/pczt_test.rs` | Update `parse_artifact` calls to match `ArtifactSummary::Zcash(...)`. |
| `tests/tests/integration_test.rs` | Update any `ArtifactSummary` construction/assertion. |

**Verification**:
```bash
cargo build --workspace
cargo test --workspace
```

**What this stage enables**: Types are centralized. `chain_id` is in the IPC message
so the bridge/signer can auto-configure. `SubmitIntentResult` is available for the
API layer. The `SignerProtocol` trait is clean (no `chain()` dead weight).

---

## Stage 2: Zcash protocol split — `common.rs`, feature gates, `ZcashSignerProtocol`

**Goal**: Create shared helpers, gate wallet deps behind a feature, introduce
`ZcashSignerProtocol` as a second `SignerProtocol` impl. `ZcashProtocol` still has
its own `SignerProtocol` impl — nothing is removed yet.

**Files touched**:

| File | Change |
|------|--------|
| `protocols/zcash/src/common.rs` | **New file.** `account_from_path()`, `decode_orchard_recipient()`, `ZCASH_COIN_TYPE`. |
| `protocols/zcash/Cargo.toml` | Expand `[features]`: `wallet` gate adds `tactix`, `tokio`, `zcash_client_backend`, `zcash_client_sqlite`, `rusqlite`, `tonic`, `reqwest`, `serde_json`, `secrecy`. Mark those deps `optional = true`. Add `tracing` as always-available dep. |
| `protocols/zcash/src/signer.rs` | **New file.** `ZcashSignerProtocol` struct with `params: LocalNetwork` and `network_type: NetworkType`. `impl SignerProtocol` with `export_viewing`, `parse_artifact`, `sign`. Move `sign_transaction_inner` and `KeyRef` from `protocol.rs` into `signer.rs` as a private helper. |
| `protocols/zcash/src/protocol.rs` | Import from `crate::common` instead of local helpers. Keep existing `impl SignerProtocol` (it still lives here for now). |
| `protocols/zcash/src/lib.rs` | Add `pub mod common;` and `pub mod signer;`. Gate `lsp_client`, `scan_actor`, `wallet_actor` behind `#[cfg(feature = "wallet")]`. Gate `create_protocol`, `ZcashStack`, `open_wallet_db` behind `#[cfg(feature = "wallet")]`. Export `ZcashSignerProtocol` unconditionally. Export `ZcashProtocol` unconditionally. |

**Verification**:
```bash
cargo build --workspace                              # default features (wallet ON)
cargo build -p paypunk-chains-zcash --no-default-features  # signer-only build
cargo test --workspace
```

**What this stage enables**: The signer-side code exists as a standalone impl.
Wallet-heavy code can be excluded for the signer app. Both `ZcashProtocol` and
`ZcashSignerProtocol` coexist — keypunkd still uses the former.

---

## Stage 3: Remove `SignerProtocol` from `ZcashProtocol`, migrate keypunkd

**Goal**: `ZcashProtocol` is wallet-only (just `impl Protocol`). `ZcashSignerProtocol`
is the sole `SignerProtocol` impl. keypunkd switches to `ZcashSignerProtocol`.

**Files touched**:

| File | Change |
|------|--------|
| `protocols/zcash/src/protocol.rs` | Remove `impl SignerProtocol for ZcashProtocol` block. Remove `sign_transaction_inner` and `KeyRef` (they live in `signer.rs` now). |
| `keypunkd/src/run.rs` | Replace `paypunk_chains_zcash::protocol::ZcashProtocol::new(...)` with `paypunk_chains_zcash::signer::ZcashSignerProtocol::new(...)`. |
| `keypunkd/src/services.rs` | No logic change — `chain_id` is already passed through from Stage 1. |
| `tests/tests/pczt_test.rs` | Update any test that called `ZcashProtocol::sign()` to use `ZcashSignerProtocol::sign()` instead. |
| `tests/tests/integration_test.rs` | Update protocol registration in `TestBuilder` to register `ZcashSignerProtocol` for keypunkd. |

**Verification**:
```bash
cargo build --workspace
cargo test --workspace
cargo test -p tests   # integration tests
```

**What this stage enables**: Clean separation. `ZcashProtocol` is wallet-only.
`ZcashSignerProtocol` is signer-only. keypunkd uses the lightweight signer impl.

---

## Stage 4: paypunkd + api signer-aware flow

**Goal**: `paypunkd` returns `KeypunkdResponse` directly (not a 4-tuple). The handler
maps `KeypunkdResponse::ArtifactPreview` to `PaypunkdResponse::SignablePreview` and
`KeypunkdResponse::ArtifactAuthorized` to `PaypunkdResponse::SignatureApproved`.
`api` returns `SubmitIntentResult` (an enum, not a 4-tuple).

**Files touched**:

| File | Change |
|------|--------|
| `paypunkd/src/usecases.rs` | `submit_intent` returns `Result<KeypunkdResponse, String>` instead of `Result<(Vec<u8>, Vec<u8>, Vec<u8>, [u8; 32]), String>`. |
| `paypunkd/src/paypunkd.rs` | `submit_intent` handler matches on `KeypunkdResponse`: `ArtifactPreview` → `PaypunkdResponse::SignablePreview`, `ArtifactAuthorized` → `PaypunkdResponse::SignatureApproved`, `Error` → `PaypunkdResponse::Error`. |
| `api/src/functions.rs` | `submit_intent` returns `Result<SubmitIntentResult, String>`. Matches `PaypunkdResponse::SignablePreview` → `SubmitIntentResult::SignablePreview`, `PaypunkdResponse::SignatureApproved` → `SubmitIntentResult::SignatureApproved`. |
| `api/src/client.rs` | `submit_intent` return type becomes `Result<SubmitIntentResult, String>`. |
| `tests/tests/integration_test.rs` | Update `submit_intent` call sites to match on `SubmitIntentResult`. |

**Verification**:
```bash
cargo build --workspace
cargo test --workspace
cargo test -p tests
```

**What this stage enables**: The API now returns `SubmitIntentResult` — consumers
can branch on `SignablePreview` (keypunkd mode) vs `SignatureApproved` (signer mode).

---

## Stage 5: TUI signer mode

**Goal**: TUI adapts to signer mode. When `signer_mode` is true: skip review screen,
skip password prompt, show "Awaiting signer..." spinner, spawn a background task
for `submit_intent` + `broadcast_transaction`, poll for completion.

**Files touched**:

| File | Change |
|------|--------|
| `tui/src/api/types.rs` | Add `skip_review: bool` to `SendReviewData`. |
| `tui/src/api/real.rs` | `RealWalletApi` stores `signer_mode: bool`. `submit_send_review` branches: if signer mode, spawn `tokio::spawn` with `submit_intent` + `broadcast_transaction`, store `oneshot::Receiver` in `pending_send_result: Mutex<Option<oneshot::Receiver<...>>>`. Return `SendReviewData { skip_review: true, ... }`. Add `poll_send_result()` method. Change `#[async_trait(?Send)]` to `#[async_trait]` on `WalletApi`. |
| `tui/src/screens/send.rs` | When `SendReviewData.skip_review` is true, skip `SendStep::Review` → jump to `SendStep::Sending` with spinner. `tick()` polls `api.poll_send_result()`. When complete, set `self.result` and `self.step = SendStep::Confirm`. No `submit_send_confirm` call. |
| `tui/src/lib.rs` | `run_tui(socket_path, shutdown, signer_mode: bool)`. Pass `signer_mode` to `RealWalletApi::connect()`. |

**Verification**:
```bash
cargo build --workspace
cargo test -p paypunk-tui
```

**What this stage enables**: TUI knows about signer mode. The `--signer` flag from
Stage 6 now has a consumer.

---

## Stage 6: CLI config switch

**Goal**: `--signer` flag on the CLI. When set, spawn the bridge instead of
keypunkd, connect paypunkd to the bridge socket, pass `signer_mode: true` to
`run_tui`.

**Files touched**:

| File | Change |
|------|--------|
| `cli/src/main.rs` | Add `signer` flag to config/args. In `ensure_daemons`: when signer mode, skip keypunkd spawn, spawn bridge instead, pass bridge socket path to paypunkd. Pass `signer_mode: true` to `run_tui()`. |
| `config/` | Add `offline_signer: bool` field to `PaypunkConfig`. |

**Verification**:
```bash
cargo build --workspace
cargo test --workspace
# Manual smoke test:
cargo run -- --signer   # should spawn bridge (not keypunkd), TUI shows signer mode
```

**What this stage enables**: The full signer-mode pipeline is operational end-to-end
(with a bridge waiting for a QR response). The existing keypunkd mode is unchanged.

---

## Stage 7: Signer app (Tauri)

**Goal**: Tauri mobile/desktop app that holds the seed, scans QR codes from the
bridge, parses `KeypunkdRequest::PreviewArtifact`, displays a real preview (extracted
from the PCZT), and signs with real Orchard proving.

**Files touched**:

| File | Change |
|------|--------|
| `signer/src-tauri/Cargo.toml` | Dependencies: `paypunk-types` (no default features), `paypunk-chains-zcash` (default-features = false), `bip39`, `postcard`, `blake2`, `tauri`, `serde`, `serde_json`. |
| `signer/src-tauri/src/signer.rs` | `SignerState` struct (seed, mnemonic, protocol, status). `handle_request()` — deserializes `KeypunkdRequest`, lazy-inits `ZcashSignerProtocol` from `chain_id`, calls `parse_artifact` for real preview. `sign()` — calls `ZcashSignerProtocol::sign()` with real Orchard proving. |
| `signer/src-tauri/src/lib.rs` | Tauri commands: `generate_seed`, `get_signer_status`, `process_scanned_qr`, `approve_and_sign`. Hardcoded test phrase for phase 1. |
| `signer/src/` | React pages: `OnboardingPage`, `ScanPage`, `PreviewPage`, `SigningPage`, `ResultPage`. |

**Verification**:
```bash
cargo build -p signer    # compiles independently
# Manual: open Tauri app, scan QR from bridge, approve, verify bridge receives response
```

**What this stage enables**: The complete offline signer experience. A real mobile
app that scans a QR, shows the transaction details, and signs with real cryptography.

---

## Dependency graph

```
Stage 1 (types)
  └── Stage 2 (zcash split)
        └── Stage 3 (keypunkd migration)
              └── Stage 4 (paypunkd + api)
                    └── Stage 5 (TUI)
                          └── Stage 6 (CLI)
  └── Stage 7 (signer app) ← depends on Stage 2 (ZcashSignerProtocol exists),
                              can start in parallel with Stage 3
```

---

## Context per stage

| Stage | Crates touched | Lines changed (est.) | Complexity |
|-------|---------------|---------------------|------------|
| 1 | 8+ | ~300 | Mechanical (move types, update imports) |
| 2 | 3 | ~250 | Additive (new files, feature gates) |
| 3 | 4 | ~100 | Removal + wiring |
| 4 | 4 | ~80 | Plumbing (enum matching) |
| 5 | 4 | ~150 | Async patterns (oneshot, spawn) |
| 6 | 2 | ~60 | Config + spawn logic |
| 7 | 4 | ~400 | New crate, real crypto |

---

## Rollback safety

Each stage is a complete, compilable state. If Stage 3 fails, Stages 1-2 are
already merged and functional. The keypunkd mode still works at every stage
through Stage 6 — the signer mode is additive until the CLI flag is wired.