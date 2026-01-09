//! YUV Conversion Utilities
//!
//! Provides conversion functions between BGRA and YUV420 color spaces.
//! YUV420 is the standard format for video encoding (VP9, H.264, etc.)
//!
//! YUV420 format:
//! - Y plane: Full resolution luminance (width * height bytes)
//! - U plane: Half resolution chrominance blue (width/2 * height/2 bytes)
//! - V plane: Half resolution chrominance red (width/2 * height/2 bytes)

/// YUV420 frame representation
///
/// Stores video frame data in YUV420 planar format, which is the standard
/// input format for video encoders like VP9.
#[derive(Debug, Clone)]
pub struct YuvFrame {
    /// Frame width in pixels
    pub width: u32,
    /// Frame height in pixels
    pub height: u32,
    /// Y plane (luminance) - full resolution
    pub y: Vec<u8>,
    /// U plane (chrominance blue, Cb) - quarter resolution
    pub u: Vec<u8>,
    /// V plane (chrominance red, Cr) - quarter resolution
    pub v: Vec<u8>,
}

impl YuvFrame {
    /// Create a new YUV frame with the given dimensions
    ///
    /// # Arguments
    /// * `width` - Frame width in pixels (must be even)
    /// * `height` - Frame height in pixels (must be even)
    ///
    /// # Returns
    /// A new YuvFrame with zeroed planes
    pub fn new(width: u32, height: u32) -> Self {
        let y_size = (width * height) as usize;
        let uv_size = y_size / 4; // YUV420: U and V are quarter resolution

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

    /// Get the Y plane as a slice
    pub fn y_plane(&self) -> &[u8] {
        &self.y
    }

    /// Get the U plane as a slice
    pub fn u_plane(&self) -> &[u8] {
        &self.u
    }

    /// Get the V plane as a slice
    pub fn v_plane(&self) -> &[u8] {
        &self.v
    }

    /// Get all planes as a contiguous byte vector (Y, then U, then V)
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.data_size());
        bytes.extend_from_slice(&self.y);
        bytes.extend_from_slice(&self.u);
        bytes.extend_from_slice(&self.v);
        bytes
    }
}

/// Convert BGRA frame to YUV420 format
///
/// Uses BT.601 color space conversion coefficients (standard for video).
///
/// # Arguments
/// * `bgra` - BGRA pixel data (4 bytes per pixel: Blue, Green, Red, Alpha)
/// * `width` - Frame width in pixels (must be even)
/// * `height` - Frame height in pixels (must be even)
///
/// # Returns
/// A YuvFrame containing the converted data, or an error if input is invalid
///
/// # Color Space Conversion (BT.601)
/// ```text
/// Y  =  0.299 * R + 0.587 * G + 0.114 * B
/// U  = -0.169 * R - 0.331 * G + 0.500 * B + 128
/// V  =  0.500 * R - 0.419 * G - 0.081 * B + 128
/// ```
pub fn bgra_to_yuv420(bgra: &[u8], width: u32, height: u32) -> Result<YuvFrame, String> {
    // Validate input
    if width == 0 || height == 0 {
        return Err("Width and height must be greater than 0".to_string());
    }
    if width % 2 != 0 || height % 2 != 0 {
        return Err("Width and height must be even for YUV420".to_string());
    }
    
    let expected_size = (width * height * 4) as usize;
    if bgra.len() != expected_size {
        return Err(format!(
            "Invalid BGRA data size: expected {} bytes, got {} bytes",
            expected_size,
            bgra.len()
        ));
    }

    bgra_to_yuv420_internal(bgra, width as usize, height as usize, width as usize * 4)
}

/// Convert BGRA frame to YUV420 format with stride support
///
/// This version handles frames with row padding (stride > width * 4).
/// Screen capture libraries like scrap often add padding for memory alignment.
///
/// # Arguments
/// * `bgra` - BGRA pixel data with potential row padding
/// * `width` - Frame width in pixels (must be even)
/// * `height` - Frame height in pixels (must be even)
/// * `stride` - Bytes per row (may be > width * 4 due to padding)
///
/// # Returns
/// A YuvFrame containing the converted data, or an error if input is invalid
pub fn bgra_to_yuv420_with_stride(bgra: &[u8], width: u32, height: u32, stride: usize) -> Result<YuvFrame, String> {
    // Validate input
    if width == 0 || height == 0 {
        return Err("Width and height must be greater than 0".to_string());
    }
    if width % 2 != 0 || height % 2 != 0 {
        return Err("Width and height must be even for YUV420".to_string());
    }
    
    let min_stride = (width * 4) as usize;
    if stride < min_stride {
        return Err(format!(
            "Stride {} is less than minimum required {} (width * 4)",
            stride, min_stride
        ));
    }
    
    let expected_size = stride * (height as usize);
    if bgra.len() < expected_size {
        return Err(format!(
            "Invalid BGRA data size: expected at least {} bytes (stride {} * height {}), got {} bytes",
            expected_size, stride, height, bgra.len()
        ));
    }

    bgra_to_yuv420_internal(bgra, width as usize, height as usize, stride)
}

/// Internal implementation of BGRA to YUV420 conversion
fn bgra_to_yuv420_internal(bgra: &[u8], width: usize, height: usize, stride: usize) -> Result<YuvFrame, String> {
    let y_size = width * height;
    let uv_size = y_size / 4;
    
    let mut y_plane = vec![0u8; y_size];
    let mut u_plane = vec![0u8; uv_size];
    let mut v_plane = vec![0u8; uv_size];

    // Convert each pixel to Y
    // For U and V, we subsample by averaging 2x2 blocks
    for row in 0..height {
        for col in 0..width {
            let bgra_idx = row * stride + col * 4;
            let y_idx = row * width + col;
            
            let b = bgra[bgra_idx] as f32;
            let g = bgra[bgra_idx + 1] as f32;
            let r = bgra[bgra_idx + 2] as f32;
            // Alpha (bgra[bgra_idx + 3]) is ignored
            
            // BT.601 conversion for Y
            let y = (0.299 * r + 0.587 * g + 0.114 * b).clamp(0.0, 255.0);
            y_plane[y_idx] = y as u8;
        }
    }

    // Calculate U and V planes with 2x2 subsampling
    for row in (0..height).step_by(2) {
        for col in (0..width).step_by(2) {
            let uv_idx = (row / 2) * (width / 2) + (col / 2);
            
            // Average the 2x2 block for U and V
            let mut r_sum = 0.0f32;
            let mut g_sum = 0.0f32;
            let mut b_sum = 0.0f32;
            
            for dy in 0..2 {
                for dx in 0..2 {
                    let bgra_idx = (row + dy) * stride + (col + dx) * 4;
                    b_sum += bgra[bgra_idx] as f32;
                    g_sum += bgra[bgra_idx + 1] as f32;
                    r_sum += bgra[bgra_idx + 2] as f32;
                }
            }
            
            // Average over 4 pixels
            let r = r_sum / 4.0;
            let g = g_sum / 4.0;
            let b = b_sum / 4.0;
            
            // BT.601 conversion for U (Cb) and V (Cr)
            let u = (-0.169 * r - 0.331 * g + 0.500 * b + 128.0).clamp(0.0, 255.0);
            let v = (0.500 * r - 0.419 * g - 0.081 * b + 128.0).clamp(0.0, 255.0);
            
            u_plane[uv_idx] = u as u8;
            v_plane[uv_idx] = v as u8;
        }
    }

    Ok(YuvFrame {
        width: width as u32,
        height: height as u32,
        y: y_plane,
        u: u_plane,
        v: v_plane,
    })
}

/// Convert YUV420 frame to RGBA format
///
/// Uses BT.601 color space conversion coefficients.
///
/// # Arguments
/// * `yuv` - YUV420 frame to convert
///
/// # Returns
/// RGBA pixel data (4 bytes per pixel: Red, Green, Blue, Alpha)
///
/// # Color Space Conversion (BT.601)
/// ```text
/// R = Y + 1.402 * (V - 128)
/// G = Y - 0.344 * (U - 128) - 0.714 * (V - 128)
/// B = Y + 1.772 * (U - 128)
/// ```
pub fn yuv420_to_rgba(yuv: &YuvFrame) -> Vec<u8> {
    let width = yuv.width as usize;
    let height = yuv.height as usize;
    let mut rgba = vec![0u8; width * height * 4];

    for row in 0..height {
        for col in 0..width {
            let y_idx = row * width + col;
            let uv_idx = (row / 2) * (width / 2) + (col / 2);
            let rgba_idx = y_idx * 4;

            let y = yuv.y[y_idx] as f32;
            let u = yuv.u.get(uv_idx).copied().unwrap_or(128) as f32 - 128.0;
            let v = yuv.v.get(uv_idx).copied().unwrap_or(128) as f32 - 128.0;

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

/// Convert BGRA to RGBA (simple channel swap)
///
/// # Arguments
/// * `bgra` - BGRA pixel data
///
/// # Returns
/// RGBA pixel data
pub fn bgra_to_rgba(bgra: &[u8]) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(bgra.len());
    for chunk in bgra.chunks(4) {
        if chunk.len() == 4 {
            rgba.push(chunk[2]); // R (was B)
            rgba.push(chunk[1]); // G
            rgba.push(chunk[0]); // B (was R)
            rgba.push(chunk[3]); // A
        }
    }
    rgba
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_yuv_frame_new() {
        let frame = YuvFrame::new(1920, 1080);
        assert_eq!(frame.width, 1920);
        assert_eq!(frame.height, 1080);
        assert_eq!(frame.y.len(), 1920 * 1080);
        assert_eq!(frame.u.len(), 1920 * 1080 / 4);
        assert_eq!(frame.v.len(), 1920 * 1080 / 4);
    }

    #[test]
    fn test_yuv_frame_data_size() {
        let frame = YuvFrame::new(1920, 1080);
        // YUV420: Y = w*h, U = w*h/4, V = w*h/4
        // Total = w*h * 1.5
        let expected = (1920 * 1080 * 3) / 2;
        assert_eq!(frame.data_size(), expected);
    }

    #[test]
    fn test_bgra_to_yuv420_invalid_size() {
        let bgra = vec![0u8; 100]; // Wrong size
        let result = bgra_to_yuv420(&bgra, 10, 10);
        assert!(result.is_err());
    }

    #[test]
    fn test_bgra_to_yuv420_odd_dimensions() {
        let bgra = vec![0u8; 9 * 9 * 4];
        let result = bgra_to_yuv420(&bgra, 9, 9);
        assert!(result.is_err());
    }

    #[test]
    fn test_bgra_to_yuv420_zero_dimensions() {
        let bgra = vec![];
        let result = bgra_to_yuv420(&bgra, 0, 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_bgra_to_yuv420_black() {
        // Black in BGRA: B=0, G=0, R=0, A=255
        let bgra = vec![0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255];
        let yuv = bgra_to_yuv420(&bgra, 2, 2).unwrap();
        
        // Black in YUV: Y=0, U=128, V=128
        for &y in &yuv.y {
            assert_eq!(y, 0, "Y should be 0 for black");
        }
        for &u in &yuv.u {
            assert_eq!(u, 128, "U should be 128 for black");
        }
        for &v in &yuv.v {
            assert_eq!(v, 128, "V should be 128 for black");
        }
    }

    #[test]
    fn test_bgra_to_yuv420_white() {
        // White in BGRA: B=255, G=255, R=255, A=255
        let bgra = vec![255, 255, 255, 255, 255, 255, 255, 255, 
                       255, 255, 255, 255, 255, 255, 255, 255];
        let yuv = bgra_to_yuv420(&bgra, 2, 2).unwrap();
        
        // White in YUV: Y=255, U=128, V=128
        for &y in &yuv.y {
            assert_eq!(y, 255, "Y should be 255 for white");
        }
        for &u in &yuv.u {
            assert_eq!(u, 128, "U should be 128 for white");
        }
        for &v in &yuv.v {
            assert_eq!(v, 128, "V should be 128 for white");
        }
    }

    #[test]
    fn test_bgra_to_yuv420_red() {
        // Red in BGRA: B=0, G=0, R=255, A=255
        let bgra = vec![0, 0, 255, 255, 0, 0, 255, 255, 
                       0, 0, 255, 255, 0, 0, 255, 255];
        let yuv = bgra_to_yuv420(&bgra, 2, 2).unwrap();
        
        // Red in YUV (BT.601): Y≈76, U≈85, V≈255
        for &y in &yuv.y {
            assert!((y as i32 - 76).abs() <= 1, "Y should be ~76 for red, got {}", y);
        }
        for &u in &yuv.u {
            assert!((u as i32 - 85).abs() <= 1, "U should be ~85 for red, got {}", u);
        }
        for &v in &yuv.v {
            assert_eq!(v, 255, "V should be 255 for red");
        }
    }

    #[test]
    fn test_yuv420_to_rgba_black() {
        let mut yuv = YuvFrame::new(2, 2);
        yuv.y = vec![0, 0, 0, 0];
        yuv.u = vec![128];
        yuv.v = vec![128];
        
        let rgba = yuv420_to_rgba(&yuv);
        
        for i in 0..4 {
            let idx = i * 4;
            assert!(rgba[idx] < 5, "R should be near 0 for black");
            assert!(rgba[idx + 1] < 5, "G should be near 0 for black");
            assert!(rgba[idx + 2] < 5, "B should be near 0 for black");
            assert_eq!(rgba[idx + 3], 255, "A should be 255");
        }
    }

    #[test]
    fn test_yuv420_to_rgba_white() {
        let mut yuv = YuvFrame::new(2, 2);
        yuv.y = vec![255, 255, 255, 255];
        yuv.u = vec![128];
        yuv.v = vec![128];
        
        let rgba = yuv420_to_rgba(&yuv);
        
        for i in 0..4 {
            let idx = i * 4;
            assert!(rgba[idx] > 250, "R should be near 255 for white");
            assert!(rgba[idx + 1] > 250, "G should be near 255 for white");
            assert!(rgba[idx + 2] > 250, "B should be near 255 for white");
            assert_eq!(rgba[idx + 3], 255, "A should be 255");
        }
    }

    #[test]
    fn test_bgra_to_rgba() {
        let bgra = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let rgba = bgra_to_rgba(&bgra);
        
        assert_eq!(rgba[0], 3); // R (was B position)
        assert_eq!(rgba[1], 2); // G
        assert_eq!(rgba[2], 1); // B (was R position)
        assert_eq!(rgba[3], 4); // A
        
        assert_eq!(rgba[4], 7);
        assert_eq!(rgba[5], 6);
        assert_eq!(rgba[6], 5);
        assert_eq!(rgba[7], 8);
    }

    #[test]
    fn test_yuv_frame_to_bytes() {
        let mut yuv = YuvFrame::new(2, 2);
        yuv.y = vec![1, 2, 3, 4];
        yuv.u = vec![5];
        yuv.v = vec![6];
        
        let bytes = yuv.to_bytes();
        assert_eq!(bytes, vec![1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn test_bgra_to_yuv420_with_stride() {
        // Create BGRA data with stride (extra padding per row)
        // 2x2 image with stride of 16 bytes per row (instead of 8)
        let width = 2u32;
        let height = 2u32;
        let stride = 16usize; // 4 extra bytes padding per row
        
        // Black pixels: B=0, G=0, R=0, A=255
        let mut bgra = vec![0u8; stride * height as usize];
        // Row 0: pixel 0, pixel 1, padding
        bgra[0..4].copy_from_slice(&[0, 0, 0, 255]); // pixel (0,0)
        bgra[4..8].copy_from_slice(&[0, 0, 0, 255]); // pixel (1,0)
        // bytes 8-15 are padding
        // Row 1: pixel 0, pixel 1, padding
        bgra[16..20].copy_from_slice(&[0, 0, 0, 255]); // pixel (0,1)
        bgra[20..24].copy_from_slice(&[0, 0, 0, 255]); // pixel (1,1)
        // bytes 24-31 are padding
        
        let yuv = bgra_to_yuv420_with_stride(&bgra, width, height, stride).unwrap();
        
        // Black in YUV: Y=0, U=128, V=128
        for &y in &yuv.y {
            assert_eq!(y, 0, "Y should be 0 for black");
        }
        for &u in &yuv.u {
            assert_eq!(u, 128, "U should be 128 for black");
        }
        for &v in &yuv.v {
            assert_eq!(v, 128, "V should be 128 for black");
        }
    }

    #[test]
    fn test_bgra_to_yuv420_with_stride_white() {
        // Create white pixels with stride
        let width = 2u32;
        let height = 2u32;
        let stride = 12usize; // 4 extra bytes padding per row
        
        let mut bgra = vec![0u8; stride * height as usize];
        // White pixels: B=255, G=255, R=255, A=255
        bgra[0..4].copy_from_slice(&[255, 255, 255, 255]);
        bgra[4..8].copy_from_slice(&[255, 255, 255, 255]);
        bgra[12..16].copy_from_slice(&[255, 255, 255, 255]);
        bgra[16..20].copy_from_slice(&[255, 255, 255, 255]);
        
        let yuv = bgra_to_yuv420_with_stride(&bgra, width, height, stride).unwrap();
        
        // White in YUV: Y=255, U=128, V=128
        for &y in &yuv.y {
            assert_eq!(y, 255, "Y should be 255 for white");
        }
        for &u in &yuv.u {
            assert_eq!(u, 128, "U should be 128 for white");
        }
        for &v in &yuv.v {
            assert_eq!(v, 128, "V should be 128 for white");
        }
    }

    #[test]
    fn test_bgra_to_yuv420_with_stride_invalid() {
        let bgra = vec![0u8; 100];
        
        // Stride too small
        let result = bgra_to_yuv420_with_stride(&bgra, 4, 4, 8);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Stride"));
        
        // Data too small for stride * height
        let result = bgra_to_yuv420_with_stride(&bgra, 4, 4, 32);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid BGRA data size"));
    }
}
