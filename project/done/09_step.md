# Step 9: Signer app (Tauri)

## Goal

Create the Tauri mobile/desktop signer app under `signer/`. The app holds the seed,
scans QR codes from the bridge, parses `KeypunkdRequest::PreviewArtifact`, displays
a real transaction preview (extracted from the PCZT), and signs with real Orchard
proving. Phase 1 uses a hardcoded test phrase.

## Files to create/change

### 1. `signer/src-tauri/Cargo.toml`

Create or update with these dependencies:

```toml
[package]
name = "paypunk-signer"
version = "0.1.0"
edition = "2021"

[dependencies]
tauri = { version = "2", features = [] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
paypunk-types = { path = "../../types", default-features = false }
paypunk-chains-zcash = { path = "../../protocols/zcash", default-features = false }
bip39 = { workspace = true }
postcard = { workspace = true }
blake2 = { workspace = true }
thiserror = { workspace = true }
tracing = { workspace = true }

[build-dependencies]
tauri-build = { version = "2", features = [] }

[lib]
name = "paypunk_signer_lib"
crate-type = ["lib", "cdylib", "staticlib"]
```

### 2. `signer/src-tauri/src/signer.rs` — New file: SignerState

```rust
use paypunk_chains_zcash::signer::ZcashSignerProtocol;
use paypunk_chains_zcash::to_local_params;
use paypunk_types::{
    ArtifactSummary, ChainId, KeypunkdRequest, KeypunkdResponse, ProtocolId, SignerProtocol,
};
use zcash_primitives::consensus::{LocalNetwork, Network};
use zcash_protocol::consensus::NetworkType;

pub struct SignerState {
    pub seed: [u8; 64],
    pub mnemonic: String,
    zcash_signer: Option<ZcashSignerProtocol>,
    pub status: SignerStatus,
}

pub enum SignerStatus {
    Idle,
    Previewing {
        raw_artifact: Vec<u8>,
        summary: ArtifactSummary,
        derivation_path: String,
        protocol: ProtocolId,
    },
    Signing,
    Signed {
        signed_artifact: Vec<u8>,
    },
    Error(String),
}

impl SignerState {
    pub fn create() -> Self {
        // Hardcoded test phrase for phase 1
        let mnemonic = "ribbon velvet ocean puzzle harvest guitar shadow ladder comfort raven spring anchor".to_string();
        let seed = bip39::Mnemonic::parse(&mnemonic)
            .expect("valid mnemonic")
            .to_seed("");

        Self {
            seed: seed.as_bytes().try_into().expect("seed is 64 bytes"),
            mnemonic,
            zcash_signer: None,
            status: SignerStatus::Idle,
        }
    }

    fn get_or_init_zcash(&mut self, chain_id: &ChainId) -> Result<&ZcashSignerProtocol, String> {
        if self.zcash_signer.is_none() {
            let network_type = match chain_id.reference.as_str() {
                "mainnet" => NetworkType::Main,
                "testnet" => NetworkType::Test,
                "regtest" => NetworkType::Regtest,
                _ => return Err(format!("unsupported zcash network: {}", chain_id.reference)),
            };
            let network = Network::from_network_type(network_type);
            let params = to_local_params(network, network_type);
            self.zcash_signer = Some(ZcashSignerProtocol::new(params, network_type));
        }
        Ok(self.zcash_signer.as_ref().unwrap())
    }

    pub fn handle_request(&mut self, request_bytes: &[u8]) -> Vec<u8> {
        let request: KeypunkdRequest = match postcard::from_bytes(request_bytes) {
            Ok(r) => r,
            Err(e) => {
                let resp = KeypunkdResponse::Error {
                    message: format!("deserialize failed: {e}"),
                };
                return postcard::to_allocvec(&resp).unwrap_or_default();
            }
        };

        let response = match request {
            KeypunkdRequest::PreviewArtifact {
                raw_artifact,
                protocol,
                chain_id,
                derivation_path,
            } => match protocol {
                ProtocolId::Zcash => {
                    let signer = match self.get_or_init_zcash(&chain_id) {
                        Ok(s) => s,
                        Err(e) => {
                            return postcard::to_allocvec(&KeypunkdResponse::Error {
                                message: e,
                            })
                            .unwrap_or_default();
                        }
                    };

                    let parsed = match signer.parse_artifact(&raw_artifact) {
                        Ok(p) => p,
                        Err(e) => {
                            return postcard::to_allocvec(&KeypunkdResponse::Error {
                                message: e,
                            })
                            .unwrap_or_default();
                        }
                    };

                    let summary: ArtifactSummary = match postcard::from_bytes(&parsed) {
                        Ok(s) => s,
                        Err(e) => {
                            return postcard::to_allocvec(&KeypunkdResponse::Error {
                                message: format!("summary deserialize: {e}"),
                            })
                            .unwrap_or_default();
                        }
                    };

                    self.status = SignerStatus::Previewing {
                        raw_artifact,
                        summary,
                        derivation_path,
                        protocol,
                    };

                    KeypunkdResponse::ArtifactPreview {
                        raw_artifact: vec![],
                        parsed_summary: parsed,
                        signature: vec![],
                        keypunkd_public_key: [0u8; 32],
                    }
                }
                ProtocolId::Ethereum => KeypunkdResponse::Error {
                    message: "Ethereum signing not yet supported in signer".to_string(),
                },
            },
            _ => KeypunkdResponse::Error {
                message: "unsupported request".to_string(),
            },
        };

        postcard::to_allocvec(&response).unwrap_or_default()
    }

    pub fn approve_and_sign(&mut self) -> Result<Vec<u8>, String> {
        let (raw_artifact, derivation_path, protocol) = match &self.status {
            SignerStatus::Previewing {
                raw_artifact,
                derivation_path,
                protocol,
                ..
            } => (raw_artifact.clone(), derivation_path.clone(), *protocol),
            _ => return Err("no preview to sign".to_string()),
        };

        self.status = SignerStatus::Signing;

        let signed = match protocol {
            ProtocolId::Zcash => {
                let signer = self
                    .zcash_signer
                    .as_ref()
                    .ok_or("zcash signer not initialized")?;
                signer.sign(&self.seed, &derivation_path, &raw_artifact)?
            }
            ProtocolId::Ethereum => {
                return Err("Ethereum signing not yet supported".to_string());
            }
        };

        self.status = SignerStatus::Signed {
            signed_artifact: signed.clone(),
        };

        Ok(signed)
    }
}
```

### 3. `signer/src-tauri/src/lib.rs` — Tauri commands

```rust
mod signer;

use signer::{SignerState, SignerStatus};
use std::sync::Mutex;
use tauri::State;

struct AppState {
    signer: Mutex<SignerState>,
}

#[tauri::command]
fn generate_seed(state: State<AppState>) -> Result<String, String> {
    let mut signer = state.signer.lock().map_err(|e| e.to_string())?;
    *signer = SignerState::create();
    Ok(signer.mnemonic.clone())
}

#[tauri::command]
fn get_signer_status(state: State<AppState>) -> String {
    let signer = state.signer.lock().unwrap();
    match &signer.status {
        SignerStatus::Idle => "idle".to_string(),
        SignerStatus::Previewing { .. } => "previewing".to_string(),
        SignerStatus::Signing => "signing".to_string(),
        SignerStatus::Signed { .. } => "signed".to_string(),
        SignerStatus::Error(e) => format!("error: {e}"),
    }
}

#[tauri::command]
fn process_scanned_qr(state: State<AppState>, qr_data: String) -> Result<String, String> {
    let mut signer = state.signer.lock().map_err(|e| e.to_string())?;
    let request_bytes = hex::decode(&qr_data).map_err(|e| format!("hex decode: {e}"))?;
    let response_bytes = signer.handle_request(&request_bytes);
    Ok(hex::encode(&response_bytes))
}

#[tauri::command]
fn approve_and_sign(state: State<AppState>) -> Result<String, String> {
    let mut signer = state.signer.lock().map_err(|e| e.to_string())?;
    let signed = signer.approve_and_sign()?;
    Ok(hex::encode(&signed))
}

#[tauri::command]
fn get_preview(state: State<AppState>) -> Result<serde_json::Value, String> {
    let signer = state.signer.lock().map_err(|e| e.to_string())?;
    match &signer.status {
        SignerStatus::Previewing { summary, .. } => {
            serde_json::to_value(summary).map_err(|e| format!("serialize: {e}"))
        }
        _ => Err("no preview available".to_string()),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_state = AppState {
        signer: Mutex::new(SignerState::create()),
    };

    tauri::Builder::default()
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            generate_seed,
            get_signer_status,
            process_scanned_qr,
            approve_and_sign,
            get_preview,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

### 4. `signer/src/` — React frontend pages

Create the React pages for the signer app:

**`signer/src/pages/OnboardingPage.tsx`** — "Generate Seed" button. Calls
`generate_seed` Tauri command. Shows mnemonic phrase. Navigate to `/scan`.

**`signer/src/pages/ScanPage.tsx`** — Barcode scanner. Captures QR code.
Calls `process_scanned_qr` with the hex-encoded QR data. On success, navigates to
`/preview`.

**`signer/src/pages/PreviewPage.tsx`** — Shows transaction details from
`get_preview`. For Zcash: shows list of outputs (address + amount) and fee. For
Ethereum: shows to, amount, fee, nonce. "Approve" and "Reject" buttons. Approve
calls `approve_and_sign`, navigates to `/result`.

**`signer/src/pages/SigningPage.tsx`** — Spinner during Orchard proving.
Shows "Signing..." text. Polls `get_signer_status` until status is "signed".
Navigates to `/result`.

**`signer/src/pages/ResultPage.tsx`** — Shows response QR code for the bridge to
scan. The QR contains the hex-encoded `KeypunkdResponse::ArtifactAuthorized`.
"Done" button navigates back to `/scan`.

### 5. `signer/package.json` and frontend config

Ensure the Tauri app has the necessary frontend dependencies:

```json
{
  "dependencies": {
    "@tauri-apps/api": "^2",
    "react": "^18",
    "react-dom": "^18",
    "react-router-dom": "^6",
    "html5-qrcode": "^2"
  }
}
```

## Acceptance criteria

1. `cargo build -p paypunk-signer` (or `cargo build` in `signer/src-tauri/`)
   succeeds.
2. The signer app compiles with `--no-default-features` on `paypunk-chains-zcash`
   (no wallet DB, no scan actor, no lightwalletd).
3. `cargo fmt --all` produces no changes.
4. `SignerState::create()` generates a hardcoded test phrase.
5. `handle_request()`:
   - Deserializes `KeypunkdRequest` from raw bytes.
   - For `PreviewArtifact` with `ProtocolId::Zcash`: lazy-inits
     `ZcashSignerProtocol` from `chain_id`, calls `parse_artifact` for real
     preview, stores `SignerStatus::Previewing` with the real `ArtifactSummary`.
   - Returns `KeypunkdResponse::ArtifactPreview` with the parsed summary.
6. `approve_and_sign()`:
   - Calls `ZcashSignerProtocol::sign()` with real Orchard proving.
   - Returns the signed PCZT bytes.
7. Tauri commands are callable from the frontend: `generate_seed`, `process_scanned_qr`,
   `approve_and_sign`, `get_preview`, `get_signer_status`.

## Context

The signer app is a separate Tauri project. It does NOT depend on the wallet
features of `paypunk-chains-zcash` — only the signer crate (`ZcashSignerProtocol`).
This keeps the binary size small for mobile.

Phase 1 uses a hardcoded test phrase. The `chain_id` from the `PreviewArtifact`
message tells the signer which network to configure (mainnet, testnet, regtest).
The signer auto-configures `ZcashSignerProtocol` from it.

The `parse_artifact` call extracts real data from the PCZT:
- Recipient addresses (decoded from Orchard raw bytes)
- Output amounts (in zatoshis)
- Fee (from Orchard value sum)

The signer app is network-agnostic — the `chain_id` from the message drives
everything. When the seed changes (e.g., randomly generated), it works on any
network.

Ethereum signing is stubbed for now — the `handle_request` returns an error for
Ethereum previews. Ethereum support can be added later by adding an
`EthereumSignerProtocol` to the signer state.

## Verification

```bash
# Build the signer app
cargo build -p paypunk-signer 2>/dev/null || cargo build --manifest-path signer/src-tauri/Cargo.toml

# Or if the signer is excluded from workspace:
cd signer/src-tauri && cargo build

cargo fmt --all
```

After verification, move this file to `./project/done/09_step.md` and commit with:

```
git add -A && git commit -m "signer: add Tauri signer app with real Orchard proving and QR-based preview"
```