use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};
use tauri_plugin_store::StoreExt;

// ── Data types ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppInfo {
    pub app_name: String,
    pub app_version: String,
    pub target_triple: String,
    pub build_profile: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GreetResult {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListItem {
    pub id: u32,
    pub title: String,
    pub description: String,
    pub category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub theme_preference: String,
    pub launch_count: u64,
    pub favourite_color: String,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimerTick {
    pub tick: u64,
}

// ── Application state ─────────────────────────────────────────────

pub struct AppState {
    pub tick_count: Mutex<u64>,
}

// ── Tauri commands ────────────────────────────────────────────────

#[tauri::command]
fn get_app_info() -> AppInfo {
    AppInfo {
        app_name: "PayPunk Signer".to_string(),
        app_version: "0.1.0".to_string(),
        target_triple: std::env::consts::ARCH.to_string()
            + "-"
            + std::env::consts::OS.to_string().as_str(),
        build_profile: if cfg!(debug_assertions) {
            "debug".to_string()
        } else {
            "release".to_string()
        },
        source: "rust".to_string(),
    }
}

#[tauri::command]
fn greet(name: String) -> GreetResult {
    GreetResult {
        message: format!("Hello, {}! Welcome to PayPunk Signer.", name),
    }
}

#[tauri::command]
fn get_list_items() -> Vec<ListItem> {
    vec![
        ListItem {
            id: 1,
            title: "Rust Item Alpha".to_string(),
            description: "Populated by Rust via tauri::command".to_string(),
            category: "rust".to_string(),
        },
        ListItem {
            id: 2,
            title: "Rust Item Beta".to_string(),
            description: "Data flows from Rust into Konsta List".to_string(),
            category: "rust".to_string(),
        },
        ListItem {
            id: 3,
            title: "Rust Item Gamma".to_string(),
            description: "Serde-serialised Vec from backend".to_string(),
            category: "rust".to_string(),
        },
        ListItem {
            id: 4,
            title: "Rust Item Delta".to_string(),
            description: "Fourth item demonstrating list population".to_string(),
            category: "rust".to_string(),
        },
    ]
}

#[tauri::command]
fn get_settings(app: AppHandle) -> Settings {
    let store = app.store("settings.json").unwrap();
    Settings {
        theme_preference: store
            .get("theme_preference")
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| "material".to_string()),
        launch_count: store
            .get("launch_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        favourite_color: store
            .get("favourite_color")
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| "#ff0000".to_string()),
        note: store
            .get("note")
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default(),
    }
}

#[tauri::command]
fn save_settings(
    app: AppHandle,
    theme_preference: Option<String>,
    favourite_color: Option<String>,
    note: Option<String>,
) -> Result<Settings, String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;

    if let Some(ref val) = theme_preference {
        store.set("theme_preference", val.clone());
    }
    if let Some(ref val) = favourite_color {
        store.set("favourite_color", val.clone());
    }
    if let Some(ref val) = note {
        store.set("note", val.clone());
    }

    store.save().map_err(|e| e.to_string())?;

    // Re-read and return
    Ok(get_settings(app))
}

// ── Timer event emitter ───────────────────────────────────────────

pub fn start_timer(app: AppHandle) {
    let app_clone = app.clone();
    std::thread::spawn(move || {
        let mut tick: u64 = 0;
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
            tick += 1;
            let _ = app_clone.emit("timer-tick", TimerTick { tick });
        }
    });
}

// ── App builder ───────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .manage(AppState {
            tick_count: Mutex::new(0),
        })
        .invoke_handler(tauri::generate_handler![
            get_app_info,
            greet,
            get_list_items,
            get_settings,
            save_settings,
        ])
        .setup(|app| {
            // Start the timer event emitter
            start_timer(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
