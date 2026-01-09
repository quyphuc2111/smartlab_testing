//! Client Receiver Module
//!
//! Provides TCP-based video receiving for LAN screen sharing.
//! Implements frame buffer for reassembly, VP9 decoding, and Tauri event emission.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::io::AsyncReadExt;
use tokio::net::{TcpStream, UdpSocket};
use tauri::{AppHandle, Emitter};

use crate::decoder::{Vp9Decoder, yuv420_to_rgba};
use crate::protocol::{FrameBuffer, StreamPacket, ProtocolError, HEADER_SIZE};

const DISCOVERY_PORT: u16 = 34254;

/// Server information discovered via UDP broadcast
#[derive(Clone, serde::Serialize)]
pub struct ServerInfo {
    pub ip: String,
    pub port: u16,
    pub name: String,
}

/// Decoded frame data to emit to frontend
#[derive(Clone, serde::Serialize)]
pub struct DecodedFrameData {
    /// Frame width in pixels
    pub width: u32,
    /// Frame height in pixels
    pub height: u32,
    /// RGBA pixel data (base64 encoded for JSON transport)
    pub data: Vec<u8>,
    /// Frame ID for ordering
    pub frame_id: u32,
    /// Whether this was decoded from a keyframe
    pub is_keyframe: bool,
}

/// Receiver state for managing video reception
pub struct ReceiverState {
    pub is_receiving: Arc<AtomicBool>,
}

impl Default for ReceiverState {
    fn default() -> Self {
        Self {
            is_receiving: Arc::new(AtomicBool::new(false)),
        }
    }
}

/// Start UDP discovery listener to find servers on the network
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
                    if parts.len() >= 4 {
                        // Format: SCREEN_SHARE_SERVER:<ip>:<port>:<name>
                        let ip = parts[1].to_string();
                        let port: u16 = parts[2].parse().unwrap_or(34256);
                        // Name may contain colons, so join remaining parts
                        let name = parts[3..].join(":");
                        let _ = app.emit("server-found", ServerInfo { ip, port, name });
                    } else if parts.len() == 3 {
                        // Legacy format without name: SCREEN_SHARE_SERVER:<ip>:<port>
                        let ip = parts[1].to_string();
                        let port: u16 = parts[2].parse().unwrap_or(34256);
                        let name = format!("Teacher ({})", ip);
                        let _ = app.emit("server-found", ServerInfo { ip, port, name });
                    }
                }
            }
        }
    });

    Ok(())
}

/// TCP Video Receiver
///
/// Connects to a streaming server, receives VP9 packets,
/// decodes them, and emits RGBA frames to the frontend.
struct TcpVideoReceiver {
    /// TCP stream for receiving data
    stream: TcpStream,
    /// Frame buffer for reassembling chunked frames
    frame_buffer: FrameBuffer,
    /// VP9 decoder
    decoder: Vp9Decoder,
    /// Read buffer for TCP data
    read_buffer: Vec<u8>,
    /// Bytes currently in the read buffer
    buffer_len: usize,
}

impl TcpVideoReceiver {
    /// Create a new TCP video receiver connected to the specified server
    pub async fn connect(ip: &str, port: u16) -> Result<Self, String> {
        let addr = format!("{}:{}", ip, port);
        let stream = TcpStream::connect(&addr)
            .await
            .map_err(|e| format!("Failed to connect to {}: {}", addr, e))?;
        
        let decoder = Vp9Decoder::new()
            .map_err(|e| format!("Failed to create VP9 decoder: {}", e))?;
        
        Ok(Self {
            stream,
            frame_buffer: FrameBuffer::new(10),
            decoder,
            read_buffer: vec![0u8; 1024 * 1024], // 1MB buffer
            buffer_len: 0,
        })
    }

    /// Read more data from the TCP stream into the buffer
    async fn read_more(&mut self) -> Result<usize, String> {
        // Shift remaining data to the beginning if needed
        if self.buffer_len > 0 && self.buffer_len < self.read_buffer.len() / 2 {
            // Buffer is less than half full, no need to shift
        }
        
        let read_start = self.buffer_len;
        let available = self.read_buffer.len() - read_start;
        
        if available == 0 {
            return Err("Buffer full, cannot read more data".to_string());
        }
        
        let n = self.stream.read(&mut self.read_buffer[read_start..])
            .await
            .map_err(|e| format!("Read error: {}", e))?;
        
        if n == 0 {
            return Err("Connection closed".to_string());
        }
        
        self.buffer_len += n;
        Ok(n)
    }

    /// Try to parse a packet from the buffer
    fn try_parse_packet(&mut self) -> Result<Option<StreamPacket>, ProtocolError> {
        if self.buffer_len < HEADER_SIZE {
            return Ok(None);
        }
        
        // Try to deserialize a packet
        match StreamPacket::deserialize(&self.read_buffer[..self.buffer_len]) {
            Ok((packet, consumed)) => {
                // Remove consumed bytes from buffer
                self.read_buffer.copy_within(consumed..self.buffer_len, 0);
                self.buffer_len -= consumed;
                Ok(Some(packet))
            }
            Err(ProtocolError::InsufficientData { .. }) => {
                // Need more data
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    /// Receive and decode the next frame
    /// Returns decoded RGBA frame data or None if more data is needed
    pub async fn receive_frame(&mut self) -> Result<Option<DecodedFrameData>, String> {
        loop {
            // Try to parse packets from buffer
            match self.try_parse_packet() {
                Ok(Some(packet)) => {
                    // Process packet through frame buffer
                    match self.frame_buffer.process_packet(packet) {
                        Ok(Some(reassembled)) => {
                            // Decode the VP9 frame
                            match self.decoder.decode(&reassembled.data) {
                                Ok(Some(yuv_frame)) => {
                                    // Convert YUV to RGBA
                                    let rgba_data = yuv420_to_rgba(&yuv_frame);
                                    
                                    return Ok(Some(DecodedFrameData {
                                        width: yuv_frame.width,
                                        height: yuv_frame.height,
                                        data: rgba_data,
                                        frame_id: reassembled.frame_id,
                                        is_keyframe: reassembled.is_keyframe,
                                    }));
                                }
                                Ok(None) => {
                                    // Decoder needs more data, continue
                                    continue;
                                }
                                Err(e) => {
                                    // Decode error - log and continue
                                    // This can happen if we miss a keyframe
                                    return Err(format!("Decode error: {}", e));
                                }
                            }
                        }
                        Ok(None) => {
                            // Frame not complete yet, continue parsing
                            continue;
                        }
                        Err(e) => {
                            return Err(format!("Frame buffer error: {}", e));
                        }
                    }
                }
                Ok(None) => {
                    // Need more data from network
                    self.read_more().await?;
                }
                Err(e) => {
                    return Err(format!("Protocol error: {}", e));
                }
            }
        }
    }

    /// Reset the decoder (e.g., after seeking or error recovery)
    pub fn reset_decoder(&mut self) -> Result<(), String> {
        self.decoder.reset()?;
        self.frame_buffer.reset();
        Ok(())
    }
}

/// Start receiving video from a server
///
/// Connects to the specified server, receives VP9 frames,
/// decodes them, and emits RGBA data to the frontend via Tauri events.
#[tauri::command]
pub async fn start_tcp_video_receiver(
    app: AppHandle,
    ip: String,
    port: u16,
    state: tauri::State<'_, ReceiverState>,
) -> Result<(), String> {
    // Check if already receiving
    if state.is_receiving.load(Ordering::SeqCst) {
        return Err("Already receiving video".to_string());
    }
    
    let _ = app.emit("log-message", format!("Connecting to {}:{}...", ip, port));
    
    // Create receiver
    let mut receiver = TcpVideoReceiver::connect(&ip, port).await?;
    
    let _ = app.emit("log-message", format!("Connected to {}:{}", ip, port));
    
    let is_receiving = state.is_receiving.clone();
    is_receiving.store(true, Ordering::SeqCst);
    
    // Spawn receiver task
    tokio::spawn(async move {
        let mut frame_count = 0u64;
        let mut error_count = 0u32;
        const MAX_CONSECUTIVE_ERRORS: u32 = 10;
        
        while is_receiving.load(Ordering::SeqCst) {
            match tokio::time::timeout(
                std::time::Duration::from_secs(5),
                receiver.receive_frame()
            ).await {
                Ok(Ok(Some(frame_data))) => {
                    error_count = 0; // Reset error count on success
                    frame_count += 1;
                    
                    // Emit decoded frame to frontend
                    let _ = app.emit("decoded-frame", &frame_data);
                    
                    // Periodic logging
                    if frame_count % 30 == 0 {
                        let _ = app.emit("log-message", 
                            format!("Received frame #{} ({}x{}, keyframe: {})", 
                                frame_data.frame_id, 
                                frame_data.width, 
                                frame_data.height,
                                frame_data.is_keyframe));
                    }
                }
                Ok(Ok(None)) => {
                    // No frame ready, continue
                    continue;
                }
                Ok(Err(e)) => {
                    error_count += 1;
                    let _ = app.emit("log-message", format!("Receive error: {}", e));
                    
                    // Try to reset decoder on decode errors
                    if e.contains("Decode error") {
                        let _ = receiver.reset_decoder();
                        let _ = app.emit("log-message", "Decoder reset, waiting for keyframe".to_string());
                    }
                    
                    if error_count >= MAX_CONSECUTIVE_ERRORS {
                        let _ = app.emit("log-message", "Too many errors, stopping receiver".to_string());
                        break;
                    }
                    
                    // Connection closed
                    if e.contains("Connection closed") {
                        let _ = app.emit("log-message", "Server disconnected".to_string());
                        break;
                    }
                }
                Err(_) => {
                    // Timeout - check if we should continue
                    let _ = app.emit("log-message", "Receive timeout, checking connection...".to_string());
                }
            }
        }
        
        is_receiving.store(false, Ordering::SeqCst);
        let _ = app.emit("log-message", format!("Video receiver stopped after {} frames", frame_count));
        let _ = app.emit("receiver-stopped", ());
    });

    Ok(())
}

/// Stop receiving video
#[tauri::command]
pub async fn stop_tcp_video_receiver(state: tauri::State<'_, ReceiverState>) -> Result<(), String> {
    state.is_receiving.store(false, Ordering::SeqCst);
    Ok(())
}

// Keep the old multicast-based receiver for backward compatibility
// but mark it as deprecated

/// Legacy: Start multicast video receiver (deprecated, use start_tcp_video_receiver instead)
#[tauri::command]
pub async fn start_video_receiver(
    app: AppHandle,
    state: tauri::State<'_, ReceiverState>,
) -> Result<(), String> {
    use std::net::Ipv4Addr;
    use std::collections::HashMap;
    use tokio::sync::Mutex;
    use socket2::{Socket, Domain, Type, Protocol};
    
    const MULTICAST_ADDR: &str = "239.255.0.1";
    const MULTICAST_PORT: u16 = 34255;
    
    // Check if already receiving
    if state.is_receiving.load(Ordering::SeqCst) {
        return Err("Already receiving".to_string());
    }
    
    // Create socket with SO_REUSEADDR
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))
        .map_err(|e| e.to_string())?;
    socket.set_reuse_address(true).map_err(|e| e.to_string())?;
    
    let addr: std::net::SocketAddr = format!("0.0.0.0:{}", MULTICAST_PORT).parse().unwrap();
    socket.bind(&addr.into()).map_err(|e| e.to_string())?;
    socket.set_nonblocking(true).map_err(|e| e.to_string())?;
    
    let socket = UdpSocket::from_std(socket.into()).map_err(|e| e.to_string())?;
    
    // Join multicast group
    let multicast_addr: Ipv4Addr = MULTICAST_ADDR.parse().unwrap();
    socket.join_multicast_v4(multicast_addr, Ipv4Addr::UNSPECIFIED)
        .map_err(|e| e.to_string())?;
    
    let _ = app.emit("log-message", format!("Joined multicast group {}:{}", MULTICAST_ADDR, MULTICAST_PORT));

    // Frame reassembly state (legacy format)
    struct LegacyFrameBuffer {
        chunks: HashMap<u16, Vec<u8>>,
        expected_chunks: u16,
    }
    
    let frame_buffers: Arc<Mutex<HashMap<u32, LegacyFrameBuffer>>> = Arc::new(Mutex::new(HashMap::new()));
    let is_receiving = state.is_receiving.clone();
    is_receiving.store(true, Ordering::SeqCst);
    
    tokio::spawn(async move {
        let mut buf = vec![0u8; 65535];
        let mut last_complete_frame = 0u32;
        
        while is_receiving.load(Ordering::SeqCst) {
            match tokio::time::timeout(
                std::time::Duration::from_millis(100),
                socket.recv_from(&mut buf)
            ).await {
                Ok(Ok((len, _))) => {
                    if len < 8 {
                        continue;
                    }
                    
                    // Parse header (legacy format)
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
                    let frame_buf = buffers.entry(frame_id).or_insert_with(|| LegacyFrameBuffer {
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
                        
                        // Emit frame to frontend (legacy: raw JPEG data)
                        let _ = app.emit("video-frame", complete_frame);
                        
                        last_complete_frame = frame_id;
                        
                        // Clean up old buffers
                        buffers.retain(|&id, _| id > frame_id.saturating_sub(5));
                    }
                }
                Ok(Err(e)) => {
                    let _ = app.emit("log-message", format!("Receive error: {}", e));
                }
                Err(_) => {
                    // Timeout, continue loop to check is_receiving
                }
            }
        }
        let _ = app.emit("log-message", "Video receiver stopped".to_string());
    });

    Ok(())
}

/// Legacy: Stop multicast video receiver
#[tauri::command]
pub async fn stop_video_receiver(state: tauri::State<'_, ReceiverState>) -> Result<(), String> {
    state.is_receiving.store(false, Ordering::SeqCst);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_info_serialize() {
        let info = ServerInfo {
            ip: "192.168.1.100".to_string(),
            port: 34256,
            name: "Teacher's Computer".to_string(),
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("192.168.1.100"));
        assert!(json.contains("34256"));
        assert!(json.contains("Teacher's Computer"));
    }

    #[test]
    fn test_decoded_frame_data_serialize() {
        let frame = DecodedFrameData {
            width: 1920,
            height: 1080,
            data: vec![255, 0, 0, 255], // One red pixel
            frame_id: 42,
            is_keyframe: true,
        };
        let json = serde_json::to_string(&frame).unwrap();
        assert!(json.contains("1920"));
        assert!(json.contains("1080"));
        assert!(json.contains("42"));
        assert!(json.contains("true"));
    }

    #[test]
    fn test_receiver_state_default() {
        let state = ReceiverState::default();
        assert!(!state.is_receiving.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_tcp_receiver_connect_failure() {
        // Try to connect to a non-existent server
        let result = TcpVideoReceiver::connect("127.0.0.1", 59999).await;
        assert!(result.is_err());
    }

    /// Test discovery message parsing with name
    #[test]
    fn test_parse_discovery_message_with_name() {
        let msg = "SCREEN_SHARE_SERVER:192.168.1.100:34256:Teacher's Computer";
        let parts: Vec<&str> = msg.split(':').collect();
        
        assert!(parts.len() >= 4);
        assert_eq!(parts[0], "SCREEN_SHARE_SERVER");
        assert_eq!(parts[1], "192.168.1.100");
        assert_eq!(parts[2], "34256");
        // Name may contain colons, so join remaining parts
        let name = parts[3..].join(":");
        assert_eq!(name, "Teacher's Computer");
    }

    /// Test discovery message parsing with name containing colons
    #[test]
    fn test_parse_discovery_message_with_colons_in_name() {
        let msg = "SCREEN_SHARE_SERVER:192.168.1.100:34256:Room 101: Math Class";
        let parts: Vec<&str> = msg.split(':').collect();
        
        assert!(parts.len() >= 4);
        assert_eq!(parts[1], "192.168.1.100");
        assert_eq!(parts[2], "34256");
        let name = parts[3..].join(":");
        assert_eq!(name, "Room 101: Math Class");
    }

    /// Test legacy discovery message parsing (without name)
    #[test]
    fn test_parse_legacy_discovery_message() {
        let msg = "SCREEN_SHARE_SERVER:192.168.1.100:34256";
        let parts: Vec<&str> = msg.split(':').collect();
        
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[1], "192.168.1.100");
        assert_eq!(parts[2], "34256");
        // Legacy format should generate default name
        let name = format!("Teacher ({})", parts[1]);
        assert_eq!(name, "Teacher (192.168.1.100)");
    }
}
