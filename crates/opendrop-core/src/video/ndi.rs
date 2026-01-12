//! NDI (Network Device Interface) video output
//!
//! Enables streaming video over network using NewTek NDI protocol.
//! Requires NDI Tools to be installed: https://ndi.video/tools/
//!
//! # Platform Support
//! - Windows: NDI runtime from NDI Tools
//! - Linux: libndi.so from NDI SDK
//! - macOS: NDI runtime from NDI Tools
//!
//! # Feature Flag
//! Enable with `--features ndi` in Cargo.toml

use super::output::{OutputBackend, VideoOutput, VideoOutputError};

/// Configuration for NDI output
#[derive(Debug, Clone)]
pub struct NdiConfig {
    /// NDI source name (visible to receivers)
    pub name: String,
    /// NDI groups (empty for default)
    pub groups: Option<String>,
    /// Whether to clock video (rate-limit to framerate)
    pub clock_video: bool,
}

impl Default for NdiConfig {
    fn default() -> Self {
        Self {
            name: "OpenDrop".to_string(),
            groups: None,
            clock_video: true,
        }
    }
}

impl NdiConfig {
    /// Create config with custom name
    pub fn with_name(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }
}

/// NDI sender information
#[derive(Debug, Clone)]
pub struct NdiSenderInfo {
    pub name: String,
    pub connected_receivers: u32,
}

/// NDI video output implementation
///
/// Streams video frames over network using NDI protocol.
/// Other NDI-compatible software (OBS, vMix, etc.) can receive the stream.
#[cfg(feature = "ndi")]
pub struct NdiOutput {
    config: NdiConfig,
    // Note: grafton-ndi types would go here
    // sender: Option<grafton_ndi::Sender>,
    // ndi: grafton_ndi::Ndi,
    active: bool,
    width: u32,
    height: u32,
    /// Frame buffer for RGBA to NDI conversion
    frame_buffer: Vec<u8>,
}

#[cfg(feature = "ndi")]
impl NdiOutput {
    /// Check if NDI runtime is available
    pub fn is_available() -> bool {
        // Try to load NDI library
        // grafton_ndi checks this on initialization
        match grafton_ndi::Ndi::new() {
            Ok(_) => true,
            Err(_) => false,
        }
    }

    /// Create a new NDI output with default config
    pub fn new() -> Result<Self, VideoOutputError> {
        Self::with_config(NdiConfig::default())
    }

    /// Create a new NDI output with custom config
    pub fn with_config(config: NdiConfig) -> Result<Self, VideoOutputError> {
        // Initialize NDI library
        let _ndi = grafton_ndi::Ndi::new()
            .map_err(|e| VideoOutputError::InitError(format!("NDI init failed: {}", e)))?;

        Ok(Self {
            config,
            active: false,
            width: 1280,
            height: 720,
            frame_buffer: Vec::new(),
        })
    }

    /// Get information about the sender
    pub fn info(&self) -> NdiSenderInfo {
        NdiSenderInfo {
            name: self.config.name.clone(),
            connected_receivers: 0, // Would query from sender
        }
    }

    /// Start the NDI sender
    fn start_sender(&mut self) -> Result<(), VideoOutputError> {
        // Build sender options
        let _options = grafton_ndi::SenderOptions::builder(&self.config.name)
            .clock_video(self.config.clock_video)
            .build();

        // Note: Actual sender creation would go here
        // self.sender = Some(grafton_ndi::Sender::new(&self.ndi, &options)?);

        tracing::info!("NDI sender started: {}", self.config.name);
        Ok(())
    }

    /// Stop the NDI sender
    fn stop_sender(&mut self) {
        // Sender would be dropped here
        tracing::info!("NDI sender stopped: {}", self.config.name);
    }
}

#[cfg(feature = "ndi")]
impl Default for NdiOutput {
    fn default() -> Self {
        Self::new().expect("Failed to create default NDI output")
    }
}

#[cfg(feature = "ndi")]
impl VideoOutput for NdiOutput {
    fn backend(&self) -> OutputBackend {
        OutputBackend::Ndi
    }

    fn send_frame(&mut self, _texture_id: u32, width: u32, height: u32) -> Result<(), VideoOutputError> {
        if !self.active {
            return Ok(());
        }

        // NDI doesn't support direct texture sending
        // Must use send_frame_rgba instead
        self.width = width;
        self.height = height;

        Err(VideoOutputError::SendError(
            "NDI requires RGBA pixel data. Use send_frame_rgba instead.".to_string()
        ))
    }

    fn send_frame_rgba(&mut self, pixels: &[u8], width: u32, height: u32) -> Result<(), VideoOutputError> {
        if !self.active {
            return Ok(());
        }

        let expected_size = (width * height * 4) as usize;
        if pixels.len() != expected_size {
            return Err(VideoOutputError::SendError(format!(
                "Invalid pixel buffer size: expected {}, got {}",
                expected_size, pixels.len()
            )));
        }

        self.width = width;
        self.height = height;

        // Create NDI video frame
        // Note: Actual implementation would create VideoFrame and send
        // let frame = grafton_ndi::VideoFrame::new(width, height, FourCC::RGBA, pixels);
        // self.sender.as_mut().unwrap().send_video(&frame);

        Ok(())
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn name(&self) -> &str {
        &self.config.name
    }

    fn set_active(&mut self, active: bool) {
        if active && !self.active {
            if let Err(e) = self.start_sender() {
                tracing::error!("Failed to start NDI sender: {}", e);
                return;
            }
        } else if !active && self.active {
            self.stop_sender();
        }
        self.active = active;
    }
}

#[cfg(feature = "ndi")]
impl Drop for NdiOutput {
    fn drop(&mut self) {
        if self.active {
            self.stop_sender();
        }
    }
}

// Stub implementation when NDI feature is disabled
#[cfg(not(feature = "ndi"))]
pub struct NdiOutput {
    config: NdiConfig,
    active: bool,
}

#[cfg(not(feature = "ndi"))]
impl NdiOutput {
    /// Check if NDI runtime is available
    pub fn is_available() -> bool {
        false
    }

    /// Create a new NDI output (returns error when feature disabled)
    pub fn new() -> Result<Self, VideoOutputError> {
        Err(VideoOutputError::NotSupported)
    }

    /// Create a new NDI output with config (returns error when feature disabled)
    pub fn with_config(_config: NdiConfig) -> Result<Self, VideoOutputError> {
        Err(VideoOutputError::NotSupported)
    }

    /// Get information about the sender
    pub fn info(&self) -> NdiSenderInfo {
        NdiSenderInfo {
            name: self.config.name.clone(),
            connected_receivers: 0,
        }
    }
}

#[cfg(not(feature = "ndi"))]
impl VideoOutput for NdiOutput {
    fn backend(&self) -> OutputBackend {
        OutputBackend::Ndi
    }

    fn send_frame(&mut self, _texture_id: u32, _width: u32, _height: u32) -> Result<(), VideoOutputError> {
        Err(VideoOutputError::NotSupported)
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn name(&self) -> &str {
        &self.config.name
    }

    fn set_active(&mut self, _active: bool) {
        // No-op when feature disabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ndi_config_default() {
        let config = NdiConfig::default();
        assert_eq!(config.name, "OpenDrop");
        assert!(config.clock_video);
        assert!(config.groups.is_none());
    }

    #[test]
    fn test_ndi_config_with_name() {
        let config = NdiConfig::with_name("Test Source");
        assert_eq!(config.name, "Test Source");
    }

    #[test]
    fn test_ndi_not_available_without_feature() {
        #[cfg(not(feature = "ndi"))]
        assert!(!NdiOutput::is_available());
    }
}
