mod signer;

use base64::Engine;
use paypunk_ipc::messages::{MAC_LEN, MSG_APPLICATION};
use paypunk_pong::PongHandler;
use paypunk_types::KeypunkdResponse;
use serde::Serialize;
use signer::{SignerState, SignerStatus};
use std::sync::Mutex;
use tauri::{Manager, State};

struct AppState {
    signer: Mutex<SignerState>,
    last_response: Mutex<Option<String>>,
}

#[derive(Serialize)]
struct ProcessResult {
    mode: String,
    response: Option<String>,
    raw_artifact_b64: Option<String>,
    preview_signature_b64: Option<String>,
    derivation_path: Option<String>,
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
fn get_encryption_key(state: State<AppState>) -> Result<[u8; 32], String> {
    let guard = state.signer.lock().map_err(|e| e.to_string())?;
    Ok(guard.server_public_key())
}

#[tauri::command]
fn generate_seed(
    state: State<AppState>,
    encrypted_password: Vec<u8>,
    ephemeral_public_key: [u8; 32],
) -> Result<Vec<u8>, String> {
    let mut guard = state.signer.lock().map_err(|e| e.to_string())?;
    guard.generate_seed(encrypted_password, ephemeral_public_key)
}

#[tauri::command]
fn restore_seed(
    state: State<AppState>,
    encrypted_mnemonic: Vec<u8>,
    encrypted_password: Vec<u8>,
    ephemeral_public_key: [u8; 32],
) -> Result<(), String> {
    let mut guard = state.signer.lock().map_err(|e| e.to_string())?;
    guard.restore_seed(encrypted_mnemonic, encrypted_password, ephemeral_public_key)
}

#[tauri::command]
fn get_signer_status(state: State<AppState>) -> Result<String, String> {
    let guard = state.signer.lock().map_err(|e| e.to_string())?;
    Ok(match guard.status() {
        SignerStatus::Idle => "idle".to_string(),
        SignerStatus::Previewing { .. } => "previewing".to_string(),
        SignerStatus::AwaitingRegistration { .. } => "awaiting_registration".to_string(),
        SignerStatus::Signing => "signing".to_string(),
        SignerStatus::Signed { .. } => "signed".to_string(),
        SignerStatus::Error(e) => format!("error: {e}"),
    })
}

#[tauri::command]
fn process_scanned_qr(
    state: State<AppState>,
    qr_data: String,
) -> Result<ProcessResult, String> {
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
        return Err(format!(
            "expected MSG_APPLICATION (0x04), got 0x{:02x}",
            bytes[0]
        ));
    }
    let payload = &bytes[1..bytes.len() - MAC_LEN];

    // Ping/pong test flow
    if payload == b"ping" {
        let handler = PongHandler;
        let response = handler.handle(&bytes)?;

        let b64 = base64::engine::general_purpose::STANDARD.encode(&response);
        *state.last_response.lock().map_err(|e| e.to_string())? = Some(b64.clone());

        return Ok(ProcessResult {
            mode: "response".to_string(),
            response: Some(b64),
            raw_artifact_b64: None,
            preview_signature_b64: None,
            derivation_path: None,
        });
    }

    // Process the request
    let mut guard = state.signer.lock().map_err(|e| e.to_string())?;
    let result = guard.handle_request(payload);

    // Store bridge-compatible response for get_response
    let b64 = base64::engine::general_purpose::STANDARD.encode(&result.response_bytes);
    *state.last_response.lock().map_err(|e| e.to_string())? = Some(b64);

    // Detect registration mode
    let is_registration = matches!(guard.status(), SignerStatus::AwaitingRegistration { .. });

    let mode = if is_registration {
        "register"
    } else {
        "preview"
    };

    Ok(ProcessResult {
        mode: mode.to_string(),
        response: None,
        raw_artifact_b64: result.raw_artifact.map(|v| {
            base64::engine::general_purpose::STANDARD.encode(&v)
        }),
        preview_signature_b64: result.preview_signature.map(|v| {
            base64::engine::general_purpose::STANDARD.encode(&v)
        }),
        derivation_path: result.derivation_path,
    })
}

#[tauri::command]
fn approve_and_sign(
    state: State<AppState>,
    encrypted_payload: Vec<u8>,
    ephemeral_public_key: [u8; 32],
    derivation_path: String,
) -> Result<String, String> {
    let mut guard = state.signer.lock().map_err(|e| e.to_string())?;
    let signed = guard.approve_and_sign(encrypted_payload, ephemeral_public_key, derivation_path)?;

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
fn delete_seed(state: State<AppState>) -> Result<(), String> {
    let mut guard = state.signer.lock().map_err(|e| e.to_string())?;
    guard.delete_seed()
}

#[tauri::command]
fn has_seed(state: State<AppState>) -> Result<bool, String> {
    let guard = state.signer.lock().map_err(|e| e.to_string())?;
    Ok(guard.has_seed())
}

#[tauri::command]
fn has_session_key(state: State<AppState>) -> Result<bool, String> {
    let guard = state.signer.lock().map_err(|e| e.to_string())?;
    Ok(guard.has_session_key())
}

#[tauri::command]
fn complete_registration(
    state: State<AppState>,
    password: String,
) -> Result<String, String> {
    let mut guard = state.signer.lock().map_err(|e| e.to_string())?;
    let response = guard.complete_registration(&password)?;

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
    let guard = state.signer.lock().map_err(|e| e.to_string())?;
    match guard.status() {
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
    tauri::Builder::default()
        .plugin(tauri_plugin_barcode_scanner::init())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .expect("failed to get app data dir");
            std::fs::create_dir_all(&data_dir).expect("failed to create app data dir");
            let signer = SignerState::create(data_dir);
            app.manage(AppState {
                signer: Mutex::new(signer),
                last_response: Mutex::new(None),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_encryption_key,
            generate_seed,
            restore_seed,
            delete_seed,
            get_signer_status,
            process_scanned_qr,
            approve_and_sign,
            has_seed,
            has_session_key,
            complete_registration,
            get_preview,
            get_response,
            generate_response_qr,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
