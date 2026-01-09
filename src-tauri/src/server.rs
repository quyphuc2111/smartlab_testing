use std::net::SocketAddr;
// use std::sync::Arc; // Unused
use tokio::net::{TcpListener, UdpSocket};
use tokio::sync::broadcast;
use tokio_tungstenite::accept_async;
use futures_util::SinkExt; // Needed for .send()
use serde::Serialize;
use local_ip_address::local_ip;

use crate::state::AppState;

const DISCOVERY_PORT: u16 = 34254;

#[derive(Clone, Serialize)]
pub struct ServerInfo {
    pub ip: String,
    pub port: u16,
}

use tauri::{AppHandle, Emitter};

pub async fn start_server(
    app: AppHandle,
    port: u16,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    // Check if server is already running
    // Scope the lock to ensure it is dropped before any await
    // RE-THINKING Logic for Block 1:
    // We need 'stop_tx' for lines below.
    
    let stop_tx = {
        let mut server_chk = state.server_stop_tx.lock().unwrap();
        if server_chk.is_some() {
            return Err("Server is already running".to_string());
        }
        let (tx, _) = broadcast::channel(1);
        *server_chk = Some(tx.clone());
        tx
    };

    // Get Local IP
    let my_local_ip = local_ip().map_err(|e| e.to_string())?;


    // 1. UDP Broadcaster Task
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
                    // let _ = app_udp.emit("log-message", "Broadcasting presence...".to_string()); // Too spammy?
                }
                _ = udp_stop_rx.recv() => {
                     let _ = app_udp.emit("log-message", "UDP Discovery stopped");
                    break;
                }
            }
        }
    });

    // 2. TCP WebSocket Server Task
    let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).await.map_err(|e| e.to_string())?;
    let header_store = state.header.clone();
    let video_tx = state.video_tx.clone();
    let mut tcp_stop_rx = stop_tx.subscribe();
    
    let app_tcp = app.clone();
    let _ = app.emit("log-message", format!("TCP Server listening on port {}", port));

    tokio::spawn(async move {
         loop {
            tokio::select! {
                res = listener.accept() => {
                    match res {
                        Ok((stream, addr)) => {
                           let _ = app_tcp.emit("log-message", format!("New client connection from: {}", addr));
                           let header_store = header_store.clone();
                            let mut rx = video_tx.subscribe();
                            
                            tokio::spawn(async move {
                                let mut ws_stream = match accept_async(stream).await {
                                    Ok(s) => s,
                                    Err(_) => return,
                                };

                                // 1. Send Header if exists
                                {
                                    let initial_header = {
                                        let lock = header_store.lock().unwrap();
                                        lock.clone()
                                    };
                                    
                                    if let Some(h) = initial_header {
                                        let _ = ws_stream.send(tokio_tungstenite::tungstenite::Message::Binary(h)).await;
                                    }
                                }
                                
                                // 2. Stream Loop
                                while let Ok(msg) = rx.recv().await {
                                    if let Err(_) = ws_stream.send(tokio_tungstenite::tungstenite::Message::Binary(msg)).await {
                                        break; 
                                    }
                                }
                            });
                        }
                        Err(_e) => { 
                             // Accept error, maybe continue?
                        }
                    }
                }
                _ = tcp_stop_rx.recv() => {
                    break;
                }
            }
        }
    });
    
    Ok(my_local_ip.to_string())
}

pub async fn stop_server(app: AppHandle, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let _ = app.emit("log-message", "Stopping server...");
    let mut server_chk = state.server_stop_tx.lock().unwrap();
    if let Some(tx) = server_chk.take() {
        let _ = tx.send(()); // Signal stop
    }
    
    // Clear header cache on stop? Maybe. 
    // let mut header = state.header.lock().unwrap();
    // *header = None;
    
    Ok(())
}
