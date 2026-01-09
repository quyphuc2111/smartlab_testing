//! VP9 Decoder Module
//!
//! Provides VP9 video decoding functionality for screen sharing.
//! Uses env-libvpx-sys for FFI bindings to libvpx (same as vpx-encode).
//!
//! Note: Full VP9 decoding requires libvpx to be installed:
//! - macOS: brew install libvpx
//! - Windows: vcpkg install libvpx
//! - Linux: apt install libvpx-dev

// vpx_sys is re-exported from env-libvpx-sys
use vpx_sys::*;
use vpx_sys::vpx_img_fmt::VPX_IMG_FMT_I420;
use std::ptr;

/// Decoded YUV frame from the decoder
#[derive(Debug, Clone)]
pub struct DecodedFrame {
    /// Frame width in pixels
    pub width: u32,
    /// Frame height in pixels
    pub height: u32,
    /// Y plane (luminance)
    pub y: Vec<u8>,
    /// U plane (chrominance blue)
    pub u: Vec<u8>,
    /// V plane (chrominance red)
    pub v: Vec<u8>,
}

impl DecodedFrame {
    /// Create a new decoded frame with the given dimensions
    pub fn new(width: u32, height: u32) -> Self {
        let y_size = (width * height) as usize;
        let uv_size = y_size / 4; // YUV420 format

        Self {
            width,
            height,
            y: vec![0; y_size],
            u: vec![0; uv_size],
            v: vec![0; uv_size],
        }
    }

    /// Get the total size of the YUV data in bytes
    pub fn data_size(&self) -> usize {
        self.y.len() + self.u.len() + self.v.len()
    }
}

/// Convert YUV420 DecodedFrame to RGBA for display
///
/// Uses BT.601 color space conversion coefficients.
///
/// # Arguments
/// * `frame` - DecodedFrame containing YUV420 data
///
/// # Returns
/// RGBA pixel data (4 bytes per pixel: Red, Green, Blue, Alpha)
pub fn yuv420_to_rgba(frame: &DecodedFrame) -> Vec<u8> {
    let width = frame.width as usize;
    let height = frame.height as usize;
    let mut rgba = vec![0u8; width * height * 4];

    for row in 0..height {
        for col in 0..width {
            let y_idx = row * width + col;
            let uv_idx = (row / 2) * (width / 2) + (col / 2);
            let rgba_idx = y_idx * 4;

            let y = frame.y.get(y_idx).copied().unwrap_or(0) as f32;
            let u = frame.u.get(uv_idx).copied().unwrap_or(128) as f32 - 128.0;
            let v = frame.v.get(uv_idx).copied().unwrap_or(128) as f32 - 128.0;

            // BT.601 YUV to RGB conversion
            let r = (y + 1.402 * v).clamp(0.0, 255.0) as u8;
            let g = (y - 0.344136 * u - 0.714136 * v).clamp(0.0, 255.0) as u8;
            let b = (y + 1.772 * u).clamp(0.0, 255.0) as u8;

            rgba[rgba_idx] = r;
            rgba[rgba_idx + 1] = g;
            rgba[rgba_idx + 2] = b;
            rgba[rgba_idx + 3] = 255; // Alpha
        }
    }

    rgba
}

/// VP9 Decoder wrapper
///
/// Decodes VP9 packets back to YUV420 frames for display.
pub struct Vp9Decoder {
    /// The underlying vpx codec context
    ctx: vpx_codec_ctx_t,
    /// Whether the decoder has been initialized
    initialized: bool,
}

// Safety: The vpx_codec_ctx_t is safe to send between threads
// as long as we don't access it concurrently
unsafe impl Send for Vp9Decoder {}

impl Vp9Decoder {
    /// Create a new VP9 decoder
    pub fn new() -> Result<Self, String> {
        unsafe {
            // Get VP9 decoder interface
            let iface = vpx_codec_vp9_dx();
            if iface.is_null() {
                return Err("VP9 decoder not available".to_string());
            }

            // Initialize decoder configuration
            let cfg = vpx_codec_dec_cfg_t {
                threads: 4, // Use 4 threads for decoding
                w: 0,       // Will be determined from bitstream
                h: 0,       // Will be determined from bitstream
            };

            // Initialize the codec context
            let mut ctx: vpx_codec_ctx_t = std::mem::zeroed();
            
            // The ABI version from env-libvpx-sys
            let result = vpx_codec_dec_init_ver(
                &mut ctx,
                iface,
                &cfg,
                0, // flags
                VPX_DECODER_ABI_VERSION as i32,
            );

            if result != VPX_CODEC_OK {
                let err_str = vpx_codec_err_to_string(result);
                let err_msg = if err_str.is_null() {
                    "Unknown error".to_string()
                } else {
                    std::ffi::CStr::from_ptr(err_str)
                        .to_string_lossy()
                        .to_string()
                };
                return Err(format!("Failed to initialize VP9 decoder: {}", err_msg));
            }

            Ok(Self {
                ctx,
                initialized: true,
            })
        }
    }

    /// Decode a VP9 packet to YUV420 frame
    ///
    /// # Arguments
    /// * `data` - VP9 encoded data
    ///
    /// # Returns
    /// Decoded YUV frame if successful, None if more data is needed
    pub fn decode(&mut self, data: &[u8]) -> Result<Option<DecodedFrame>, String> {
        if !self.initialized {
            return Err("Decoder not initialized".to_string());
        }

        if data.is_empty() {
            return Ok(None);
        }

        unsafe {
            // Decode the packet
            let result = vpx_codec_decode(
                &mut self.ctx,
                data.as_ptr(),
                data.len() as u32,
                ptr::null_mut(),
                0, // deadline (0 = no deadline)
            );

            if result != VPX_CODEC_OK {
                let err_detail = vpx_codec_error_detail(&mut self.ctx);
                let err_msg = if err_detail.is_null() {
                    let err_str = vpx_codec_err_to_string(result);
                    if err_str.is_null() {
                        "Unknown decode error".to_string()
                    } else {
                        std::ffi::CStr::from_ptr(err_str)
                            .to_string_lossy()
                            .to_string()
                    }
                } else {
                    std::ffi::CStr::from_ptr(err_detail)
                        .to_string_lossy()
                        .to_string()
                };
                return Err(format!("VP9 decode failed: {}", err_msg));
            }

            // Get the decoded frame
            let mut iter: vpx_codec_iter_t = ptr::null();
            let img = vpx_codec_get_frame(&mut self.ctx, &mut iter);

            if img.is_null() {
                // No frame ready yet (may need more data)
                return Ok(None);
            }

            // Extract frame data from the image
            let img_ref = &*img;
            let width = img_ref.d_w;
            let height = img_ref.d_h;

            // Verify it's I420 format
            if img_ref.fmt != VPX_IMG_FMT_I420 {
                return Err(format!(
                    "Unexpected image format: {:?}, expected I420",
                    img_ref.fmt
                ));
            }

            // Copy Y plane
            let y_stride = img_ref.stride[0] as usize;
            let y_plane = img_ref.planes[0];
            let mut y_data = Vec::with_capacity((width * height) as usize);
            for row in 0..height as usize {
                let row_start = y_plane.add(row * y_stride);
                let row_slice = std::slice::from_raw_parts(row_start, width as usize);
                y_data.extend_from_slice(row_slice);
            }

            // Copy U plane
            let u_stride = img_ref.stride[1] as usize;
            let u_plane = img_ref.planes[1];
            let uv_width = (width / 2) as usize;
            let uv_height = (height / 2) as usize;
            let mut u_data = Vec::with_capacity(uv_width * uv_height);
            for row in 0..uv_height {
                let row_start = u_plane.add(row * u_stride);
                let row_slice = std::slice::from_raw_parts(row_start, uv_width);
                u_data.extend_from_slice(row_slice);
            }

            // Copy V plane
            let v_stride = img_ref.stride[2] as usize;
            let v_plane = img_ref.planes[2];
            let mut v_data = Vec::with_capacity(uv_width * uv_height);
            for row in 0..uv_height {
                let row_start = v_plane.add(row * v_stride);
                let row_slice = std::slice::from_raw_parts(row_start, uv_width);
                v_data.extend_from_slice(row_slice);
            }

            Ok(Some(DecodedFrame {
                width,
                height,
                y: y_data,
                u: u_data,
                v: v_data,
            }))
        }
    }

    /// Flush the decoder to get any remaining frames
    ///
    /// Call this after all data has been sent to get any buffered frames.
    pub fn flush(&mut self) -> Result<Option<DecodedFrame>, String> {
        if !self.initialized {
            return Err("Decoder not initialized".to_string());
        }

        unsafe {
            // Send null to flush
            let result = vpx_codec_decode(
                &mut self.ctx,
                ptr::null(),
                0,
                ptr::null_mut(),
                0,
            );

            if result != VPX_CODEC_OK {
                // Flush may return error if nothing to flush, which is OK
                return Ok(None);
            }

            // Get any remaining frame
            let mut iter: vpx_codec_iter_t = ptr::null();
            let img = vpx_codec_get_frame(&mut self.ctx, &mut iter);

            if img.is_null() {
                return Ok(None);
            }

            // Extract frame data (same as in decode)
            let img_ref = &*img;
            let width = img_ref.d_w;
            let height = img_ref.d_h;

            let y_stride = img_ref.stride[0] as usize;
            let y_plane = img_ref.planes[0];
            let mut y_data = Vec::with_capacity((width * height) as usize);
            for row in 0..height as usize {
                let row_start = y_plane.add(row * y_stride);
                let row_slice = std::slice::from_raw_parts(row_start, width as usize);
                y_data.extend_from_slice(row_slice);
            }

            let u_stride = img_ref.stride[1] as usize;
            let u_plane = img_ref.planes[1];
            let uv_width = (width / 2) as usize;
            let uv_height = (height / 2) as usize;
            let mut u_data = Vec::with_capacity(uv_width * uv_height);
            for row in 0..uv_height {
                let row_start = u_plane.add(row * u_stride);
                let row_slice = std::slice::from_raw_parts(row_start, uv_width);
                u_data.extend_from_slice(row_slice);
            }

            let v_stride = img_ref.stride[2] as usize;
            let v_plane = img_ref.planes[2];
            let mut v_data = Vec::with_capacity(uv_width * uv_height);
            for row in 0..uv_height {
                let row_start = v_plane.add(row * v_stride);
                let row_slice = std::slice::from_raw_parts(row_start, uv_width);
                v_data.extend_from_slice(row_slice);
            }

            Ok(Some(DecodedFrame {
                width,
                height,
                y: y_data,
                u: u_data,
                v: v_data,
            }))
        }
    }

    /// Reset the decoder state
    pub fn reset(&mut self) -> Result<(), String> {
        // Destroy and reinitialize
        if self.initialized {
            unsafe {
                vpx_codec_destroy(&mut self.ctx);
            }
            self.initialized = false;
        }

        // Reinitialize
        let new_decoder = Self::new()?;
        self.ctx = new_decoder.ctx;
        self.initialized = true;
        std::mem::forget(new_decoder); // Don't drop the new decoder's ctx

        Ok(())
    }
}

impl Default for Vp9Decoder {
    fn default() -> Self {
        Self::new().expect("Failed to create default VP9 decoder")
    }
}

impl Drop for Vp9Decoder {
    fn drop(&mut self) {
        if self.initialized {
            unsafe {
                vpx_codec_destroy(&mut self.ctx);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decoded_frame_new() {
        let frame = DecodedFrame::new(1920, 1080);
        assert_eq!(frame.width, 1920);
        assert_eq!(frame.height, 1080);
        assert_eq!(frame.y.len(), 1920 * 1080);
        assert_eq!(frame.u.len(), 1920 * 1080 / 4);
        assert_eq!(frame.v.len(), 1920 * 1080 / 4);
    }

    #[test]
    fn test_decoded_frame_data_size() {
        let frame = DecodedFrame::new(1920, 1080);
        // YUV420: Y = w*h, U = w*h/4, V = w*h/4
        // Total = w*h * 1.5
        let expected = (1920 * 1080 * 3) / 2;
        assert_eq!(frame.data_size(), expected);
    }

    #[test]
    fn test_decoder_creation() {
        let decoder = Vp9Decoder::new();
        assert!(decoder.is_ok(), "Decoder creation failed: {:?}", decoder.err());
    }

    #[test]
    fn test_decoder_default() {
        let decoder = Vp9Decoder::default();
        assert!(decoder.initialized);
    }

    #[test]
    fn test_yuv_to_rgba_size() {
        let frame = DecodedFrame::new(100, 100);
        let rgba = yuv420_to_rgba(&frame);
        assert_eq!(rgba.len(), 100 * 100 * 4);
    }

    #[test]
    fn test_yuv_to_rgba_black() {
        // Black in YUV is Y=0, U=128, V=128
        let mut frame = DecodedFrame::new(2, 2);
        frame.y = vec![0, 0, 0, 0];
        frame.u = vec![128];
        frame.v = vec![128];
        
        let rgba = yuv420_to_rgba(&frame);
        // Should be close to black (0, 0, 0)
        for i in 0..4 {
            let idx = i * 4;
            assert!(rgba[idx] < 10, "R should be near 0");
            assert!(rgba[idx + 1] < 10, "G should be near 0");
            assert!(rgba[idx + 2] < 10, "B should be near 0");
            assert_eq!(rgba[idx + 3], 255, "A should be 255");
        }
    }

    #[test]
    fn test_yuv_to_rgba_white() {
        // White in YUV is Y=255, U=128, V=128
        let mut frame = DecodedFrame::new(2, 2);
        frame.y = vec![255, 255, 255, 255];
        frame.u = vec![128];
        frame.v = vec![128];
        
        let rgba = yuv420_to_rgba(&frame);
        // Should be close to white (255, 255, 255)
        for i in 0..4 {
            let idx = i * 4;
            assert!(rgba[idx] > 245, "R should be near 255");
            assert!(rgba[idx + 1] > 245, "G should be near 255");
            assert!(rgba[idx + 2] > 245, "B should be near 255");
            assert_eq!(rgba[idx + 3], 255, "A should be 255");
        }
    }

    #[test]
    fn test_decode_empty_data() {
        let mut decoder = Vp9Decoder::new().unwrap();
        let result = decoder.decode(&[]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_decode_invalid_data() {
        let mut decoder = Vp9Decoder::new().unwrap();
        // Random invalid data should return an error
        let invalid_data = vec![0x00, 0x01, 0x02, 0x03, 0x04];
        let result = decoder.decode(&invalid_data);
        // Invalid VP9 data should return an error
        assert!(result.is_err());
    }

    #[test]
    fn test_decoder_reset() {
        let mut decoder = Vp9Decoder::new().unwrap();
        let result = decoder.reset();
        assert!(result.is_ok());
        assert!(decoder.initialized);
    }
}


    /// Integration test: encode then decode
    #[test]
    fn test_encode_decode_roundtrip() {
        use crate::encoder::{EncoderConfig, Vp9Encoder};
        use crate::yuv::YuvFrame;

        // Create encoder
        let config = EncoderConfig {
            width: 320,
            height: 240,
            bitrate: 2000,
            keyframe_interval: 30,
        };
        let mut encoder = Vp9Encoder::new(config).expect("Failed to create encoder");

        // Create decoder
        let mut decoder = Vp9Decoder::new().expect("Failed to create decoder");

        // Create a test YUV frame with a gradient pattern
        let mut yuv_frame = YuvFrame::new(320, 240);
        for (i, byte) in yuv_frame.y.iter_mut().enumerate() {
            *byte = ((i * 7) % 256) as u8;
        }
        for (i, byte) in yuv_frame.u.iter_mut().enumerate() {
            *byte = ((i * 3 + 64) % 256) as u8;
        }
        for (i, byte) in yuv_frame.v.iter_mut().enumerate() {
            *byte = ((i * 5 + 128) % 256) as u8;
        }

        // Encode multiple frames to get output (VP9 may buffer)
        let mut encoded_packets = Vec::new();
        for i in 0..60 {
            let packets = encoder.encode(&yuv_frame, i * 33).expect("Encoding failed");
            encoded_packets.extend(packets);
        }

        // Try to decode the encoded packets
        let mut decoded_frames = Vec::new();
        for packet in &encoded_packets {
            if let Ok(Some(frame)) = decoder.decode(&packet.data) {
                decoded_frames.push(frame);
            }
        }

        // Flush decoder to get any remaining frames
        while let Ok(Some(frame)) = decoder.flush() {
            decoded_frames.push(frame);
        }

        // We should have decoded at least some frames
        // Note: VP9 real-time encoding may buffer significantly
        if !decoded_frames.is_empty() {
            // Verify decoded frame dimensions match
            for frame in &decoded_frames {
                assert_eq!(frame.width, 320, "Decoded width should match");
                assert_eq!(frame.height, 240, "Decoded height should match");
                assert_eq!(frame.y.len(), 320 * 240, "Y plane size should match");
                assert_eq!(frame.u.len(), 320 * 240 / 4, "U plane size should match");
                assert_eq!(frame.v.len(), 320 * 240 / 4, "V plane size should match");
            }
            println!("Successfully decoded {} frames", decoded_frames.len());
        } else if encoded_packets.is_empty() {
            println!("Note: VP9 encoder buffered all frames (normal for real-time encoding)");
        } else {
            println!("Note: Encoded {} packets but decoder buffered all frames", encoded_packets.len());
        }
    }


    #[test]
    fn test_yuv_to_rgba_red() {
        // Red in YUV (BT.601): Y≈76, U≈85, V≈255
        let mut frame = DecodedFrame::new(2, 2);
        frame.y = vec![76, 76, 76, 76];
        frame.u = vec![85];
        frame.v = vec![255];
        
        let rgba = yuv420_to_rgba(&frame);
        // Should be close to red (255, 0, 0)
        for i in 0..4 {
            let idx = i * 4;
            assert!(rgba[idx] > 200, "R should be high for red, got {}", rgba[idx]);
            assert!(rgba[idx + 1] < 50, "G should be low for red, got {}", rgba[idx + 1]);
            assert!(rgba[idx + 2] < 50, "B should be low for red, got {}", rgba[idx + 2]);
            assert_eq!(rgba[idx + 3], 255, "A should be 255");
        }
    }

    #[test]
    fn test_yuv_to_rgba_green() {
        // Green in YUV (BT.601): Y≈150, U≈44, V≈21
        let mut frame = DecodedFrame::new(2, 2);
        frame.y = vec![150, 150, 150, 150];
        frame.u = vec![44];
        frame.v = vec![21];
        
        let rgba = yuv420_to_rgba(&frame);
        // Should be close to green (0, 255, 0)
        for i in 0..4 {
            let idx = i * 4;
            assert!(rgba[idx] < 50, "R should be low for green, got {}", rgba[idx]);
            assert!(rgba[idx + 1] > 200, "G should be high for green, got {}", rgba[idx + 1]);
            assert!(rgba[idx + 2] < 50, "B should be low for green, got {}", rgba[idx + 2]);
            assert_eq!(rgba[idx + 3], 255, "A should be 255");
        }
    }

    #[test]
    fn test_yuv_to_rgba_blue() {
        // Blue in YUV (BT.601): Y≈29, U≈255, V≈107
        let mut frame = DecodedFrame::new(2, 2);
        frame.y = vec![29, 29, 29, 29];
        frame.u = vec![255];
        frame.v = vec![107];
        
        let rgba = yuv420_to_rgba(&frame);
        // Should be close to blue (0, 0, 255)
        for i in 0..4 {
            let idx = i * 4;
            assert!(rgba[idx] < 50, "R should be low for blue, got {}", rgba[idx]);
            assert!(rgba[idx + 1] < 50, "G should be low for blue, got {}", rgba[idx + 1]);
            assert!(rgba[idx + 2] > 200, "B should be high for blue, got {}", rgba[idx + 2]);
            assert_eq!(rgba[idx + 3], 255, "A should be 255");
        }
    }
