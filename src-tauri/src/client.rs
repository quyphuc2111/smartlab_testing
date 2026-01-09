use tauri::Emitter;
use tokio::net::UdpSocket;
use serde::Serialize;
use crate::server::ServerInfo;

const DISCOVERY_PORT: u16 = 34254;

#[derive(Clone, Serialize)]
struct ServerFoundEvent {
    ip: String,
    port: u16,
}

#[tauri::command]
pub async fn start_discovery(app_handle: tauri::AppHandle) -> Result<(), String> {
    let socket = UdpSocket::bind(format!("0.0.0.0:{}", DISCOVERY_PORT)).await.map_err(|e| e.to_string())?;
    
    // Check if we can reuse port? SO_REUSEADDR might be needed if multiple clients on same machine?
    // For now standard bind. If bind fails, maybe another client or server is running on this machine using that port.
    // Actually, Server uses `send_to` so it doesn't bind to the discovery port (it binds to random).
    // Client binds to discovery port.
    // If multiple Clients? Only one can bind.
    // For true "multicast" or shared broadcast listening, we need `socket2` with `SO_REUSEADDR`.
    // Let's assume one client per machine for basic implementation, or I can add `socket2` logic if needed. 
    // Given usage "similar to RustDesk", usually one instance.
    
    tokio::spawn(async move {
        let mut buf = [0u8; 1024];
        loop {
            match socket.recv_from(&mut buf).await {
                Ok((size, _addr)) => {
                    let msg = String::from_utf8_lossy(&buf[..size]);
                     if msg.starts_with("SCREEN_SHARE_SERVER:") {
                         let parts: Vec<&str> = msg.split(':').collect();
                         if parts.len() == 3 {
                             if let Ok(port) = parts[2].parse::<u16>() {
                                 let info = ServerInfo {
                                     ip: parts[1].to_string(),
                                     port,
                                 };
                                 let _ = app_handle.emit("log-message", format!("Discovered Server at {}:{}", info.ip, info.port));
                                 let _ = app_handle.emit("server-found", info);
                             }
                         }
                     }
                }
                Err(_) => break,
            }
        }
    });

    Ok(())
}
