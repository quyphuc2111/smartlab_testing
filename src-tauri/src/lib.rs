pub mod server;
pub mod client;
pub mod state;

use state::AppState;

#[tauri::command]
async fn start_server_cmd(app: tauri::AppHandle, port: u16, state: tauri::State<'_, AppState>) -> Result<String, String> {
    server::start_server(app, port, state).await
}

#[tauri::command]
async fn stop_server_cmd(app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<(), String> {
    server::stop_server(app, state).await
}

#[tauri::command]
async fn send_video_header(header: Vec<u8>, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let mut h = state.header.lock().unwrap();
    *h = Some(header);
    Ok(())
}

#[tauri::command]
async fn send_video_chunk(chunk: Vec<u8>, state: tauri::State<'_, AppState>) -> Result<(), String> {
    // Send to broadcast channel. If no subscribers (no students), it just drops.
    let _ = state.video_tx.send(chunk);
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            start_server_cmd,
            stop_server_cmd,
            send_video_header,
            send_video_chunk,
            client::start_discovery
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
