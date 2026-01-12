//! v4l2loopback video output for Linux
//!
//! Sends frames to a v4l2loopback device for capture by OBS, VLC, etc.
//!
//! ## Setup
//! ```bash
//! # Install v4l2loopback (Arch Linux)
//! sudo pacman -S v4l2loopback-dkms
//!
//! # Load module with virtual device
//! sudo modprobe v4l2loopback devices=1 video_nr=10 card_label="OpenDrop"
//!
//! # Verify device exists
//! ls /dev/video10
//! ```

use std::fs::OpenOptions;
use std::io::Write as IoWrite;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

use v4l::capability::Flags;
use v4l::video::Output as V4lOutput;
use v4l::{Device, Format, FourCC};

use super::output::{OutputBackend, VideoOutput, VideoOutputError};

/// v4l2loopback output configuration
#[derive(Debug, Clone)]
pub struct V4l2Config {
    /// Device path (e.g., /dev/video10)
    pub device_path: PathBuf,
    /// Output width
    pub width: u32,
    /// Output height
    pub height: u32,
}

impl Default for V4l2Config {
    fn default() -> Self {
        Self {
            device_path: PathBuf::from("/dev/video10"),
            width: 1920,
            height: 1080,
        }
    }
}

/// v4l2loopback video output
pub struct V4l2Output {
    /// File handle for writing frames
    file: std::fs::File,
    width: u32,
    height: u32,
    active: bool,
    name: String,
    /// Buffer for YUYV conversion
    yuyv_buffer: Vec<u8>,
}

impl V4l2Output {
    /// Create a new v4l2loopback output
    pub fn new(config: V4l2Config) -> Result<Self, VideoOutputError> {
        // First check if device exists and is a v4l2loopback device
        let device = Device::with_path(&config.device_path)
            .map_err(|e| VideoOutputError::InitError(format!(
                "Failed to open v4l2 device {:?}: {}. Make sure v4l2loopback is loaded: sudo modprobe v4l2loopback devices=1 video_nr=10",
                config.device_path, e
            )))?;

        // Check capabilities
        let caps = device.query_caps()
            .map_err(|e| VideoOutputError::InitError(format!("Failed to query device caps: {}", e)))?;

        if !caps.capabilities.contains(Flags::VIDEO_OUTPUT) {
            return Err(VideoOutputError::InitError(format!(
                "Device {:?} is not a video output device (not a v4l2loopback device)",
                config.device_path
            )));
        }

        // Set output format (YUYV is widely supported)
        let format = Format::new(config.width, config.height, FourCC::new(b"YUYV"));
        device.set_format(&format)
            .map_err(|e| VideoOutputError::InitError(format!(
                "Failed to set v4l2 format: {}",
                e
            )))?;

        // Drop device handle and open file for raw writing
        drop(device);

        // Open device file for writing
        let file = OpenOptions::new()
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&config.device_path)
            .map_err(|e| VideoOutputError::InitError(format!(
                "Failed to open device for writing: {}", e
            )))?;

        // Pre-allocate YUYV buffer (2 bytes per pixel)
        let yuyv_buffer = vec![0u8; (config.width * config.height * 2) as usize];

        let name = format!("v4l2:{}", config.device_path.display());

        tracing::info!(
            "Opened v4l2loopback device: {} ({}x{} YUYV)",
            config.device_path.display(),
            config.width,
            config.height
        );

        Ok(Self {
            file,
            width: config.width,
            height: config.height,
            active: true,
            name,
            yuyv_buffer,
        })
    }

    /// List available v4l2loopback devices
    pub fn list_devices() -> Vec<V4l2DeviceInfo> {
        let mut devices = Vec::new();

        // Check common loopback device paths
        for i in 0..20 {
            let path = PathBuf::from(format!("/dev/video{}", i));
            if path.exists() {
                if let Ok(device) = Device::with_path(&path) {
                    // Check if it's an output device (loopback)
                    if let Ok(caps) = device.query_caps() {
                        // v4l2loopback devices typically have VIDEO_OUTPUT capability
                        if caps.capabilities.contains(Flags::VIDEO_OUTPUT) {
                            devices.push(V4l2DeviceInfo {
                                path: path.clone(),
                                name: caps.card,
                                driver: caps.driver,
                            });
                        }
                    }
                }
            }
        }

        devices
    }

    /// Convert RGBA pixels to YUYV format
    fn rgba_to_yuyv(rgba: &[u8], yuyv: &mut [u8], width: u32, height: u32) {
        let pixels = (width * height) as usize;

        for i in 0..pixels / 2 {
            let rgba_idx = i * 2 * 4; // 2 pixels, 4 bytes each
            let yuyv_idx = i * 4;     // 2 pixels, 2 bytes each (4 total)

            if rgba_idx + 7 >= rgba.len() || yuyv_idx + 3 >= yuyv.len() {
                break;
            }

            // First pixel
            let r1 = rgba[rgba_idx] as f32;
            let g1 = rgba[rgba_idx + 1] as f32;
            let b1 = rgba[rgba_idx + 2] as f32;

            // Second pixel
            let r2 = rgba[rgba_idx + 4] as f32;
            let g2 = rgba[rgba_idx + 5] as f32;
            let b2 = rgba[rgba_idx + 6] as f32;

            // Convert to YUV (BT.601)
            let y1 = (0.299 * r1 + 0.587 * g1 + 0.114 * b1) as u8;
            let y2 = (0.299 * r2 + 0.587 * g2 + 0.114 * b2) as u8;

            // Average U and V for the two pixels
            let u = ((-0.169 * r1 - 0.331 * g1 + 0.5 * b1 + 128.0) +
                     (-0.169 * r2 - 0.331 * g2 + 0.5 * b2 + 128.0)) / 2.0;
            let v = ((0.5 * r1 - 0.419 * g1 - 0.081 * b1 + 128.0) +
                     (0.5 * r2 - 0.419 * g2 - 0.081 * b2 + 128.0)) / 2.0;

            yuyv[yuyv_idx] = y1;
            yuyv[yuyv_idx + 1] = u.clamp(0.0, 255.0) as u8;
            yuyv[yuyv_idx + 2] = y2;
            yuyv[yuyv_idx + 3] = v.clamp(0.0, 255.0) as u8;
        }
    }
}

impl VideoOutput for V4l2Output {
    fn backend(&self) -> OutputBackend {
        OutputBackend::V4l2Loopback
    }

    fn send_frame(&mut self, _texture_id: u32, _width: u32, _height: u32) -> Result<(), VideoOutputError> {
        // Texture-based sending requires OpenGL readback first
        // This would be called from the renderer with actual pixel data via send_frame_rgba
        Err(VideoOutputError::SendError(
            "Direct texture sending not supported for v4l2. Use send_frame_rgba instead.".to_string()
        ))
    }

    fn send_frame_rgba(&mut self, pixels: &[u8], width: u32, height: u32) -> Result<(), VideoOutputError> {
        if !self.active {
            return Ok(());
        }

        // Validate dimensions
        if width != self.width || height != self.height {
            return Err(VideoOutputError::SendError(format!(
                "Frame size mismatch: got {}x{}, expected {}x{}",
                width, height, self.width, self.height
            )));
        }

        let expected_size = (width * height * 4) as usize;
        if pixels.len() != expected_size {
            return Err(VideoOutputError::SendError(format!(
                "Invalid pixel buffer size: got {}, expected {}",
                pixels.len(), expected_size
            )));
        }

        // Convert RGBA to YUYV
        Self::rgba_to_yuyv(pixels, &mut self.yuyv_buffer, width, height);

        // Write to v4l2 device file
        self.file.write_all(&self.yuyv_buffer)
            .map_err(|e| VideoOutputError::SendError(format!("Failed to write frame: {}", e)))?;

        Ok(())
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn set_active(&mut self, active: bool) {
        self.active = active;
    }
}

/// Information about a v4l2 device
#[derive(Debug, Clone)]
pub struct V4l2DeviceInfo {
    pub path: PathBuf,
    pub name: String,
    pub driver: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rgba_to_yuyv_conversion() {
        // Test with small 2x1 image (minimum for YUYV)
        let rgba = vec![
            255, 0, 0, 255,   // Red pixel
            0, 255, 0, 255,   // Green pixel
        ];
        let mut yuyv = vec![0u8; 4];

        V4l2Output::rgba_to_yuyv(&rgba, &mut yuyv, 2, 1);

        // Y values should be different for red vs green
        assert!(yuyv[0] != yuyv[2], "Y values should differ for red and green");
    }

    #[test]
    fn test_rgba_to_yuyv_black_pixels() {
        // Black pixels: RGB (0,0,0) -> Y=16 (BT.601 studio swing)
        let rgba = vec![
            0, 0, 0, 255,
            0, 0, 0, 255,
        ];
        let mut yuyv = vec![0u8; 4];

        V4l2Output::rgba_to_yuyv(&rgba, &mut yuyv, 2, 1);

        // Y should be around 16 for black (BT.601 limited range)
        assert!(yuyv[0] < 30, "Black pixel Y should be low, got {}", yuyv[0]);
        assert!(yuyv[2] < 30, "Black pixel Y should be low, got {}", yuyv[2]);
    }

    #[test]
    fn test_rgba_to_yuyv_white_pixels() {
        // White pixels: RGB (255,255,255) -> Y=235 (BT.601 studio swing)
        let rgba = vec![
            255, 255, 255, 255,
            255, 255, 255, 255,
        ];
        let mut yuyv = vec![0u8; 4];

        V4l2Output::rgba_to_yuyv(&rgba, &mut yuyv, 2, 1);

        // Y should be around 235 for white (BT.601 limited range)
        assert!(yuyv[0] > 200, "White pixel Y should be high, got {}", yuyv[0]);
        assert!(yuyv[2] > 200, "White pixel Y should be high, got {}", yuyv[2]);
    }

    #[test]
    fn test_rgba_to_yuyv_gray_neutral_chroma() {
        // Gray pixels should have neutral chroma (U=V≈128)
        let rgba = vec![
            128, 128, 128, 255,
            128, 128, 128, 255,
        ];
        let mut yuyv = vec![0u8; 4];

        V4l2Output::rgba_to_yuyv(&rgba, &mut yuyv, 2, 1);

        // U and V should be near 128 for gray
        let u = yuyv[1];
        let v = yuyv[3];
        assert!((120..136).contains(&u), "Gray U should be ~128, got {}", u);
        assert!((120..136).contains(&v), "Gray V should be ~128, got {}", v);
    }

    #[test]
    fn test_rgba_to_yuyv_4x2_image() {
        // Test a larger image (4x2 pixels = 8 YUYV bytes per row * 2 rows = 16 bytes)
        let rgba = vec![
            // Row 0
            255, 0, 0, 255,     // Red
            0, 255, 0, 255,     // Green
            0, 0, 255, 255,     // Blue
            255, 255, 0, 255,   // Yellow
            // Row 1
            0, 255, 255, 255,   // Cyan
            255, 0, 255, 255,   // Magenta
            128, 128, 128, 255, // Gray
            0, 0, 0, 255,       // Black
        ];
        let mut yuyv = vec![0u8; 16]; // 4 pixels * 2 bytes/pixel * 2 rows

        V4l2Output::rgba_to_yuyv(&rgba, &mut yuyv, 4, 2);

        // Just verify no panic and output is filled
        assert_eq!(yuyv.len(), 16);
        // Red should have high Y
        assert!(yuyv[0] > 50, "Red Y should be meaningful");
        // Black (last row, last pixel pair) should have low Y
        assert!(yuyv[14] < 50, "Black Y should be low");
    }

    #[test]
    fn test_v4l2_config_default() {
        let config = V4l2Config::default();
        assert_eq!(config.device_path, std::path::PathBuf::from("/dev/video10"));
        assert_eq!(config.width, 1920);
        assert_eq!(config.height, 1080);
    }

    #[test]
    fn test_list_devices() {
        // This test just ensures the function doesn't crash
        let devices = V4l2Output::list_devices();
        println!("Found {} v4l2loopback devices", devices.len());
        for dev in &devices {
            println!("  {:?}: {} ({})", dev.path, dev.name, dev.driver);
        }
    }
}
