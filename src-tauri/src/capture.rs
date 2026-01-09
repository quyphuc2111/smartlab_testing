use scrap::{Capturer, Display};
use std::io::ErrorKind::WouldBlock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use tauri::{AppHandle, Emitter};

use crate::yuv::bgra_to_yuv420_with_stride;
use crate::encoder::{Vp9Encoder, EncoderConfig, EncodedPacket};

pub struct CaptureState {
    pub is_capturing: Arc<AtomicBool>,
}

impl Default for CaptureState {
    fn default() -> Self {
        Self {
            is_capturing: Arc::new(AtomicBool::new(false)),
        }
    }
}

pub fn get_displays() -> Result<Vec<DisplayInfo>, String> {
    let displays = Display::all().map_err(|e| e.to_string())?;
    Ok(displays
        .iter()
        .enumerate()
        .map(|(i, d)| DisplayInfo {
            index: i,
            width: d.width(),
            height: d.height(),
            name: format!("Display {}", i + 1),
        })
        .collect())
}

#[derive(Clone, serde::Serialize)]
pub struct DisplayInfo {
    pub index: usize,
    pub width: usize,
    pub height: usize,
    pub name: String,
}

/// Convert quality percentage (0-100) to bitrate in kbps
/// Low (0-33): 500 kbps
/// Medium (34-66): 2000 kbps
/// High (67-100): 5000 kbps
fn quality_to_bitrate(quality: u8) -> u32 {
    match quality {
        0..=33 => 500,
        34..=66 => 2000,
        _ => 5000,
    }
}

pub fn start_capture(
    app: AppHandle,
    display_index: usize,
    fps: u32,
    quality: u8,
    video_tx: broadcast::Sender<Vec<u8>>,
    is_capturing: Arc<AtomicBool>,
) -> Result<(), String> {
    is_capturing.store(true, Ordering::SeqCst);
    let bitrate = quality_to_bitrate(quality);
    let _ = app.emit("log-message", format!("Starting VP9 capture @ {}fps, bitrate {}kbps", fps, bitrate));

    let frame_duration = Duration::from_millis(1000 / fps as u64);

    std::thread::spawn(move || {
        // Create capturer inside the thread
        let displays = match Display::all() {
            Ok(d) => d,
            Err(e) => {
                let _ = app.emit("log-message", format!("Failed to get displays: {}", e));
                return;
            }
        };
        
        let display = match displays.into_iter().nth(display_index) {
            Some(d) => d,
            None => {
                let _ = app.emit("log-message", "Display not found".to_string());
                return;
            }
        };

        let width = display.width();
        let height = display.height();
        
        // Ensure dimensions are even for YUV420
        let enc_width = (width / 2) * 2;
        let enc_height = (height / 2) * 2;
        
        let mut capturer = match Capturer::new(display) {
            Ok(c) => c,
            Err(e) => {
                let _ = app.emit("log-message", format!("Failed to create capturer: {}", e));
                return;
            }
        };

        // Initialize VP9 encoder
        let encoder_config = EncoderConfig {
            width: enc_width as u32,
            height: enc_height as u32,
            bitrate: quality_to_bitrate(quality),
            keyframe_interval: 30, // Keyframe every 30 frames
        };
        
        let mut encoder = match Vp9Encoder::new(encoder_config) {
            Ok(e) => e,
            Err(e) => {
                let _ = app.emit("log-message", format!("Failed to create VP9 encoder: {}", e));
                return;
            }
        };

        let _ = app.emit("log-message", format!("VP9 capture started: {}x{} (encoded: {}x{})", width, height, enc_width, enc_height));

        let start_time = Instant::now();
        let mut frame_count: u64 = 0;

        while is_capturing.load(Ordering::SeqCst) {
            match capturer.frame() {
                Ok(frame) => {
                    // scrap returns frames with stride (row padding for memory alignment)
                    // Calculate stride from actual frame size
                    let frame_len = frame.len();
                    let stride = frame_len / height;
                    
                    // Convert BGRA to YUV420 with stride support
                    let yuv_frame = match bgra_to_yuv420_with_stride(&frame, enc_width as u32, enc_height as u32, stride) {
                        Ok(yuv) => yuv,
                        Err(e) => {
                            let _ = app.emit("log-message", format!("YUV conversion error: {}", e));
                            continue;
                        }
                    };

                    // Calculate PTS in milliseconds
                    let pts = start_time.elapsed().as_millis() as u64;

                    // Encode with VP9
                    match encoder.encode(&yuv_frame, pts) {
                        Ok(packets) => {
                            for packet in packets {
                                // Send encoded VP9 packet with metadata
                                let packet_data = serialize_encoded_packet(&packet);
                                let _ = video_tx.send(packet_data);
                            }
                        }
                        Err(e) => {
                            let _ = app.emit("log-message", format!("VP9 encoding error: {}", e));
                        }
                    }

                    frame_count += 1;
                    if frame_count % 30 == 0 {
                        let elapsed = start_time.elapsed().as_secs_f64();
                        let actual_fps = frame_count as f64 / elapsed;
                        let _ = app.emit("log-message", format!("Captured {} frames, {:.1} fps", frame_count, actual_fps));
                    }
                }
                Err(ref e) if e.kind() == WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(e) => {
                    let _ = app.emit("log-message", format!("Capture error: {}", e));
                    break;
                }
            }
            std::thread::sleep(frame_duration);
        }
        let _ = app.emit("log-message", "VP9 capture stopped".to_string());
    });

    Ok(())
}

/// Serialize an encoded packet for transmission
/// Format: [4 bytes: pts] [1 byte: flags] [4 bytes: data_len] [data...]
/// flags: bit 0 = is_keyframe
fn serialize_encoded_packet(packet: &EncodedPacket) -> Vec<u8> {
    let mut data = Vec::with_capacity(9 + packet.data.len());
    
    // PTS (4 bytes, big-endian)
    data.extend_from_slice(&(packet.pts as u32).to_be_bytes());
    
    // Flags (1 byte)
    let flags: u8 = if packet.is_keyframe { 0x01 } else { 0x00 };
    data.push(flags);
    
    // Data length (4 bytes, big-endian)
    data.extend_from_slice(&(packet.data.len() as u32).to_be_bytes());
    
    // VP9 data
    data.extend_from_slice(&packet.data);
    
    data
}

/// Deserialize an encoded packet from transmission format
/// Returns (pts, is_keyframe, vp9_data)
pub fn deserialize_encoded_packet(data: &[u8]) -> Result<(u64, bool, Vec<u8>), String> {
    if data.len() < 9 {
        return Err("Packet too small".to_string());
    }
    
    // PTS (4 bytes)
    let pts = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as u64;
    
    // Flags (1 byte)
    let is_keyframe = (data[4] & 0x01) != 0;
    
    // Data length (4 bytes)
    let data_len = u32::from_be_bytes([data[5], data[6], data[7], data[8]]) as usize;
    
    if data.len() < 9 + data_len {
        return Err(format!("Packet data incomplete: expected {} bytes, got {}", data_len, data.len() - 9));
    }
    
    let vp9_data = data[9..9 + data_len].to_vec();
    
    Ok((pts, is_keyframe, vp9_data))
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoder::EncodedPacket;

    #[test]
    fn test_quality_to_bitrate_low() {
        assert_eq!(quality_to_bitrate(0), 500);
        assert_eq!(quality_to_bitrate(20), 500);
        assert_eq!(quality_to_bitrate(33), 500);
    }

    #[test]
    fn test_quality_to_bitrate_medium() {
        assert_eq!(quality_to_bitrate(34), 2000);
        assert_eq!(quality_to_bitrate(50), 2000);
        assert_eq!(quality_to_bitrate(66), 2000);
    }

    #[test]
    fn test_quality_to_bitrate_high() {
        assert_eq!(quality_to_bitrate(67), 5000);
        assert_eq!(quality_to_bitrate(80), 5000);
        assert_eq!(quality_to_bitrate(100), 5000);
    }

    #[test]
    fn test_serialize_deserialize_packet() {
        let packet = EncodedPacket {
            data: vec![1, 2, 3, 4, 5],
            is_keyframe: true,
            pts: 12345,
        };

        let serialized = serialize_encoded_packet(&packet);
        let (pts, is_keyframe, data) = deserialize_encoded_packet(&serialized).unwrap();

        assert_eq!(pts, 12345);
        assert!(is_keyframe);
        assert_eq!(data, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_serialize_deserialize_non_keyframe() {
        let packet = EncodedPacket {
            data: vec![10, 20, 30],
            is_keyframe: false,
            pts: 999,
        };

        let serialized = serialize_encoded_packet(&packet);
        let (pts, is_keyframe, data) = deserialize_encoded_packet(&serialized).unwrap();

        assert_eq!(pts, 999);
        assert!(!is_keyframe);
        assert_eq!(data, vec![10, 20, 30]);
    }

    #[test]
    fn test_deserialize_packet_too_small() {
        let data = vec![0, 1, 2, 3, 4, 5, 6, 7]; // Only 8 bytes, need at least 9
        let result = deserialize_encoded_packet(&data);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("too small"));
    }

    #[test]
    fn test_deserialize_packet_incomplete_data() {
        // Header says 100 bytes of data, but only 5 provided
        let mut data = vec![0, 0, 0, 1]; // pts = 1
        data.push(0x01); // flags = keyframe
        data.extend_from_slice(&100u32.to_be_bytes()); // data_len = 100
        data.extend_from_slice(&[1, 2, 3, 4, 5]); // only 5 bytes of data

        let result = deserialize_encoded_packet(&data);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("incomplete"));
    }

    #[test]
    fn test_serialize_empty_packet() {
        let packet = EncodedPacket {
            data: vec![],
            is_keyframe: false,
            pts: 0,
        };

        let serialized = serialize_encoded_packet(&packet);
        assert_eq!(serialized.len(), 9); // 4 + 1 + 4 + 0

        let (pts, is_keyframe, data) = deserialize_encoded_packet(&serialized).unwrap();
        assert_eq!(pts, 0);
        assert!(!is_keyframe);
        assert!(data.is_empty());
    }

    #[test]
    fn test_serialize_large_pts() {
        let packet = EncodedPacket {
            data: vec![1],
            is_keyframe: true,
            pts: u32::MAX as u64, // Max u32 value
        };

        let serialized = serialize_encoded_packet(&packet);
        let (pts, is_keyframe, data) = deserialize_encoded_packet(&serialized).unwrap();

        assert_eq!(pts, u32::MAX as u64);
        assert!(is_keyframe);
        assert_eq!(data, vec![1]);
    }
}
