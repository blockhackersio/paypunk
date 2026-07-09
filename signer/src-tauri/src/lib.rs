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
