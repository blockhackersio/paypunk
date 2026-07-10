mod signer;

use base64::Engine;
use paypunk_ipc::messages::{MAC_LEN, MSG_APPLICATION};
use paypunk_pong::PongHandler;
use serde::Serialize;
use signer::{SignerState, SignerStatus};
use std::sync::Mutex;
use tauri::State;

struct AppState {
    signer: Mutex<SignerState>,
    /// Base64-encoded bridge response (for display as QR on the result page).
    /// Set by process_scanned_qr (ping/pong) or approve_and_sign (real signing).
    last_response: Mutex<Option<String>>,
}

#[derive(Serialize)]
struct ProcessResult {
    /// "preview" = real signing flow, navigate to /preview
    /// "response" = immediate response (ping/pong), navigate to /result
    mode: String,
    /// Base64-encoded bridge response (present when mode == "response")
    response: Option<String>,
}

fn generate_qr_svg(data: &str) -> Result<String, String> {
    let code = qrcode::QrCode::with_error_correction_level(data.as_bytes(), qrcode::EcLevel::L)
        .map_err(|e| format!("QR generation failed: {e}"))?;
    Ok(code
        .render()
        .min_dimensions(300, 300)
        .dark_color(qrcode::render::svg::Color("#000000"))
        .light_color(qrcode::render::svg::Color("#ffffff"))
        .build())
}

#[tauri::command]
fn generate_seed(state: State<AppState>) -> Result<String, String> {
    let mut signer = state.signer.lock().map_err(|e| e.to_string())?;
    *signer = SignerState::create();
    Ok(signer.mnemonic.clone())
}

#[tauri::command]
fn get_signer_status(state: State<AppState>) -> Result<String, String> {
    let signer = state.signer.lock().map_err(|e| e.to_string())?;
    Ok(match &signer.status {
        SignerStatus::Idle => "idle".to_string(),
        SignerStatus::Previewing { .. } => "previewing".to_string(),
        SignerStatus::Signing => "signing".to_string(),
        SignerStatus::Signed { .. } => "signed".to_string(),
        SignerStatus::Error(e) => format!("error: {e}"),
    })
}

#[tauri::command]
fn process_scanned_qr(state: State<AppState>, qr_data: String) -> Result<ProcessResult, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&qr_data)
        .map_err(|e| format!("base64 decode: {e}"))?;

    // Strip IPC frame wrapper: [MSG_APPLICATION] [payload] [32-byte MAC]
    if bytes.len() < 1 + MAC_LEN {
        return Err(format!(
            "frame too short: {} bytes, need at least {}",
            bytes.len(),
            1 + MAC_LEN
        ));
    }
    if bytes[0] != MSG_APPLICATION {
        return Err(format!("expected MSG_APPLICATION (0x04), got 0x{:02x}", bytes[0]));
    }
    let payload = &bytes[1..bytes.len() - MAC_LEN];

    // Ping/pong test flow: payload is raw bytes "ping"
    if payload == b"ping" {
        let handler = PongHandler;
        let response = handler.handle(&bytes)?;

        let b64 = base64::engine::general_purpose::STANDARD.encode(&response);
        *state.last_response.lock().map_err(|e| e.to_string())? = Some(b64.clone());

        return Ok(ProcessResult {
            mode: "response".to_string(),
            response: Some(b64),
        });
    }

    // Real signing flow: payload is postcard-serialized KeypunkdRequest
    let mut signer = state.signer.lock().map_err(|e| e.to_string())?;
    let response_bytes = signer.handle_request(payload);

    // Store the bridge-compatible response for later (approve_and_sign will
    // overwrite it with the signed artifact response)
    let b64 = base64::engine::general_purpose::STANDARD.encode(&response_bytes);
    *state.last_response.lock().map_err(|e| e.to_string())? = Some(b64);

    Ok(ProcessResult {
        mode: "preview".to_string(),
        response: None,
    })
}

#[tauri::command]
fn approve_and_sign(state: State<AppState>) -> Result<String, String> {
    let mut signer = state.signer.lock().map_err(|e| e.to_string())?;
    let signed = signer.approve_and_sign()?;

    // Wrap as bridge-compatible response: [0x00] [postcard KeypunkdResponse]
    let response = paypunk_types::KeypunkdResponse::ArtifactAuthorized {
        signed_artifact: signed,
    };
    let postcard_bytes =
        postcard::to_allocvec(&response).map_err(|e| format!("serialize: {e}"))?;

    let mut frame = Vec::with_capacity(1 + postcard_bytes.len());
    frame.push(0x00);
    frame.extend_from_slice(&postcard_bytes);

    let b64 = base64::engine::general_purpose::STANDARD.encode(&frame);
    *state.last_response.lock().map_err(|e| e.to_string())? = Some(b64.clone());

    Ok(b64)
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

#[tauri::command]
fn get_response(state: State<AppState>) -> Result<String, String> {
    let resp = state
        .last_response
        .lock()
        .map_err(|e| e.to_string())?
        .clone();
    resp.ok_or_else(|| "no response available".to_string())
}

#[tauri::command]
fn generate_response_qr(response_b64: String) -> Result<String, String> {
    generate_qr_svg(&response_b64)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_state = AppState {
        signer: Mutex::new(SignerState::create()),
        last_response: Mutex::new(None),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_barcode_scanner::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            generate_seed,
            get_signer_status,
            process_scanned_qr,
            approve_and_sign,
            get_preview,
            get_response,
            generate_response_qr,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
