pub mod server;
pub mod client;
pub mod state;
pub mod capture;

use state::AppState;
use capture::{CaptureState, DisplayInfo};

#[tauri::command]
async fn start_server_cmd(app: tauri::AppHandle, port: u16, state: tauri::State<'_, AppState>) -> Result<String, String> {
    server::start_server(app, port, state).await
}

#[tauri::command]
async fn stop_server_cmd(app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<(), String> {
    server::stop_server(app, state).await
}

#[tauri::command]
fn get_displays() -> Result<Vec<DisplayInfo>, String> {
    capture::get_displays()
}

#[tauri::command]
fn start_capture(
    app: tauri::AppHandle,
    display_index: usize,
    fps: u32,
    quality: u8,
    state: tauri::State<'_, AppState>,
    capture_state: tauri::State<'_, CaptureState>,
) -> Result<(), String> {
    capture::start_capture(
        app,
        display_index,
        fps,
        quality,
        state.video_tx.clone(),
        capture_state.is_capturing.clone(),
    )
}

#[tauri::command]
fn stop_capture(capture_state: tauri::State<'_, CaptureState>) {
    capture_state.is_capturing.store(false, std::sync::atomic::Ordering::SeqCst);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .manage(CaptureState::default())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            start_server_cmd,
            stop_server_cmd,
            get_displays,
            start_capture,
            stop_capture,
            client::start_discovery
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
