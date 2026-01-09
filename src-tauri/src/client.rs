use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use tauri::{AppHandle, Emitter};

const DISCOVERY_PORT: u16 = 34254;
const MULTICAST_ADDR: &str = "239.255.0.1";
const MULTICAST_PORT: u16 = 34255;

#[derive(Clone, serde::Serialize)]
pub struct ServerInfo {
    pub ip: String,
    pub port: u16,
}

#[tauri::command]
pub async fn start_discovery(app: AppHandle) -> Result<(), String> {
    let socket = UdpSocket::bind(format!("0.0.0.0:{}", DISCOVERY_PORT))
        .await
        .map_err(|e| e.to_string())?;
    
    let _ = socket.set_broadcast(true);

    tokio::spawn(async move {
        let mut buf = [0u8; 1024];
        loop {
            if let Ok((len, _addr)) = socket.recv_from(&mut buf).await {
                let msg = String::from_utf8_lossy(&buf[..len]);
                if msg.starts_with("SCREEN_SHARE_SERVER:") {
                    let parts: Vec<&str> = msg.split(':').collect();
                    if parts.len() == 3 {
                        let ip = parts[1].to_string();
                        let port: u16 = parts[2].parse().unwrap_or(8080);
                        let _ = app.emit("server-found", ServerInfo { ip, port });
                    }
                }
            }
        }
    });

    Ok(())
}

// Frame reassembly state
struct FrameBuffer {
    chunks: HashMap<u16, Vec<u8>>,
    expected_chunks: u16,
}

#[tauri::command]
pub async fn start_video_receiver(app: AppHandle) -> Result<(), String> {
    let socket = UdpSocket::bind(format!("0.0.0.0:{}", MULTICAST_PORT))
        .await
        .map_err(|e| e.to_string())?;
    
    // Join multicast group
    let multicast_addr: Ipv4Addr = MULTICAST_ADDR.parse().unwrap();
    socket.join_multicast_v4(multicast_addr, Ipv4Addr::UNSPECIFIED)
        .map_err(|e| e.to_string())?;
    
    let _ = app.emit("log-message", format!("Joined multicast group {}:{}", MULTICAST_ADDR, MULTICAST_PORT));

    let frame_buffers: Arc<Mutex<HashMap<u32, FrameBuffer>>> = Arc::new(Mutex::new(HashMap::new()));
    
    tokio::spawn(async move {
        let mut buf = vec![0u8; 65535];
        let mut last_complete_frame = 0u32;
        
        loop {
            match socket.recv_from(&mut buf).await {
                Ok((len, _)) => {
                    if len < 8 {
                        continue;
                    }
                    
                    // Parse header
                    let frame_id = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
                    let chunk_index = u16::from_be_bytes([buf[4], buf[5]]);
                    let total_chunks = u16::from_be_bytes([buf[6], buf[7]]);
                    let chunk_data = buf[8..len].to_vec();
                    
                    // Skip old frames
                    if frame_id <= last_complete_frame {
                        continue;
                    }
                    
                    let mut buffers = frame_buffers.lock().await;
                    
                    // Get or create frame buffer
                    let frame_buf = buffers.entry(frame_id).or_insert_with(|| FrameBuffer {
                        chunks: HashMap::new(),
                        expected_chunks: total_chunks,
                    });
                    
                    frame_buf.chunks.insert(chunk_index, chunk_data);
                    
                    // Check if frame is complete
                    if frame_buf.chunks.len() == frame_buf.expected_chunks as usize {
                        // Reassemble frame
                        let mut complete_frame = Vec::new();
                        for i in 0..total_chunks {
                            if let Some(chunk) = frame_buf.chunks.get(&i) {
                                complete_frame.extend_from_slice(chunk);
                            }
                        }
                        
                        // Emit frame to frontend
                        let _ = app.emit("video-frame", complete_frame);
                        
                        last_complete_frame = frame_id;
                        
                        // Clean up old buffers
                        buffers.retain(|&id, _| id > frame_id.saturating_sub(5));
                    }
                }
                Err(e) => {
                    let _ = app.emit("log-message", format!("Receive error: {}", e));
                }
            }
        }
    });

    Ok(())
}

#[tauri::command]
pub async fn stop_video_receiver() -> Result<(), String> {
    // The receiver will be stopped when the app closes
    // For proper cleanup, we'd need to track the task handle
    Ok(())
}
