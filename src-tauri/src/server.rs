use std::net::SocketAddr;
use tokio::net::UdpSocket;
use tokio::sync::broadcast;
use serde::Serialize;
use local_ip_address::local_ip;
use tauri::{AppHandle, Emitter};

const DISCOVERY_PORT: u16 = 34254;
const MULTICAST_ADDR: &str = "239.255.0.1";
const MULTICAST_PORT: u16 = 34255;
const MAX_UDP_PAYLOAD: usize = 65000; // Safe UDP payload size

#[derive(Clone, Serialize)]
pub struct ServerInfo {
    pub ip: String,
    pub port: u16,
}

pub async fn start_server(
    app: AppHandle,
    port: u16,
    state: tauri::State<'_, crate::state::AppState>,
) -> Result<String, String> {
    // Check if server is already running
    let stop_tx = {
        let mut server_chk = state.server_stop_tx.lock().unwrap();
        if server_chk.is_some() {
            return Err("Server is already running".to_string());
        }
        let (tx, _) = broadcast::channel(1);
        *server_chk = Some(tx.clone());
        tx
    };

    let my_local_ip = local_ip().map_err(|e| e.to_string())?;

    // 1. UDP Discovery Broadcaster
    let udp_socket = UdpSocket::bind("0.0.0.0:0").await.map_err(|e| e.to_string())?;
    let _ = udp_socket.set_broadcast(true);
    let broadcast_addr: SocketAddr = format!("255.255.255.255:{}", DISCOVERY_PORT).parse().unwrap();
    let discovery_msg = format!("SCREEN_SHARE_SERVER:{}:{}", my_local_ip, port);
    
    let mut udp_stop_rx = stop_tx.subscribe();
    let app_udp = app.clone();
    let _ = app.emit("log-message", format!("UDP Discovery started at {}:{}", my_local_ip, port));

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(1)) => {
                    let _ = udp_socket.send_to(discovery_msg.as_bytes(), broadcast_addr).await;
                }
                _ = udp_stop_rx.recv() => {
                    let _ = app_udp.emit("log-message", "UDP Discovery stopped");
                    break;
                }
            }
        }
    });

    // 2. UDP Multicast Video Streamer
    let multicast_socket = UdpSocket::bind("0.0.0.0:0").await.map_err(|e| e.to_string())?;
    let multicast_addr: SocketAddr = format!("{}:{}", MULTICAST_ADDR, MULTICAST_PORT).parse().unwrap();
    
    let video_rx = state.video_tx.subscribe();
    let mut stream_stop_rx = stop_tx.subscribe();
    let app_stream = app.clone();
    
    let _ = app.emit("log-message", format!("Multicast streaming to {}:{}", MULTICAST_ADDR, MULTICAST_PORT));

    tokio::spawn(async move {
        let mut rx = video_rx;
        let mut frame_count = 0u64;
        
        loop {
            tokio::select! {
                result = rx.recv() => {
                    match result {
                        Ok(frame_data) => {
                            frame_count += 1;
                            
                            // Split large frames into chunks
                            let chunks: Vec<&[u8]> = frame_data.chunks(MAX_UDP_PAYLOAD - 8).collect();
                            let total_chunks = chunks.len() as u16;
                            
                            for (i, chunk) in chunks.iter().enumerate() {
                                // Header: frame_id (4 bytes) + chunk_index (2 bytes) + total_chunks (2 bytes)
                                let mut packet = Vec::with_capacity(8 + chunk.len());
                                packet.extend_from_slice(&(frame_count as u32).to_be_bytes());
                                packet.extend_from_slice(&(i as u16).to_be_bytes());
                                packet.extend_from_slice(&total_chunks.to_be_bytes());
                                packet.extend_from_slice(chunk);
                                
                                let _ = multicast_socket.send_to(&packet, multicast_addr).await;
                            }
                            
                            if frame_count % 30 == 1 {
                                let _ = app_stream.emit("log-message", format!("Streamed frame #{} ({} bytes, {} chunks)", frame_count, frame_data.len(), total_chunks));
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            let _ = app_stream.emit("log-message", format!("Warning: Skipped {} frames", n));
                        }
                        Err(_) => break,
                    }
                }
                _ = stream_stop_rx.recv() => {
                    let _ = app_stream.emit("log-message", "Multicast streaming stopped");
                    break;
                }
            }
        }
    });
    
    Ok(my_local_ip.to_string())
}

pub async fn stop_server(app: AppHandle, state: tauri::State<'_, crate::state::AppState>) -> Result<(), String> {
    let _ = app.emit("log-message", "Stopping server...");
    let mut server_chk = state.server_stop_tx.lock().unwrap();
    if let Some(tx) = server_chk.take() {
        let _ = tx.send(());
    }
    Ok(())
}
