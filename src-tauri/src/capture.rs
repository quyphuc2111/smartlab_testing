use scrap::{Capturer, Display};
use std::io::ErrorKind::WouldBlock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tauri::{AppHandle, Emitter};

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

pub fn start_capture(
    app: AppHandle,
    display_index: usize,
    fps: u32,
    quality: u8,
    video_tx: broadcast::Sender<Vec<u8>>,
    is_capturing: Arc<AtomicBool>,
) -> Result<(), String> {
    is_capturing.store(true, Ordering::SeqCst);
    let _ = app.emit("log-message", format!("Starting capture @ {}fps, quality {}%", fps, quality));

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
        
        let mut capturer = match Capturer::new(display) {
            Ok(c) => c,
            Err(e) => {
                let _ = app.emit("log-message", format!("Failed to create capturer: {}", e));
                return;
            }
        };

        let _ = app.emit("log-message", format!("Capture started: {}x{}", width, height));

        while is_capturing.load(Ordering::SeqCst) {
            match capturer.frame() {
                Ok(frame) => {
                    // Convert BGRA to RGBA
                    let mut rgba = Vec::with_capacity(frame.len());
                    for chunk in frame.chunks(4) {
                        rgba.push(chunk[2]); // R
                        rgba.push(chunk[1]); // G
                        rgba.push(chunk[0]); // B
                        rgba.push(chunk[3]); // A
                    }

                    // Encode to JPEG for smaller size
                    if let Some(jpeg_data) = encode_jpeg(&rgba, width as u32, height as u32, quality) {
                        let _ = video_tx.send(jpeg_data);
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
        let _ = app.emit("log-message", "Capture stopped".to_string());
    });

    Ok(())
}

fn encode_jpeg(rgba: &[u8], width: u32, height: u32, quality: u8) -> Option<Vec<u8>> {
    use image::{ImageBuffer, Rgba};
    
    let img: ImageBuffer<Rgba<u8>, _> = ImageBuffer::from_raw(width, height, rgba.to_vec())?;
    let rgb_img = image::DynamicImage::ImageRgba8(img).to_rgb8();
    
    let mut jpeg_data = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg_data, quality);
    encoder.encode_image(&rgb_img).ok()?;
    
    Some(jpeg_data)
}
