//! Video output abstraction
//!
//! Provides a common interface for various video output backends:
//! - Window (default, OpenGL direct rendering)
//! - v4l2loopback (Linux virtual camera for OBS/VLC)
//! - Spout (Windows texture sharing)
//! - NDI (network video streaming)
//! - PipeWire (Linux native video)

use thiserror::Error;

#[derive(Error, Debug)]
pub enum VideoOutputError {
    #[error("Output initialization failed: {0}")]
    InitError(String),
    #[error("Frame send failed: {0}")]
    SendError(String),
    #[error("Output not supported on this platform")]
    NotSupported,
}

/// Video output backend type
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum OutputBackend {
    /// NDI (cross-platform network video)
    Ndi,
    /// PipeWire (Linux native)
    PipeWire,
    /// Spout (Windows texture sharing)
    Spout,
    /// v4l2loopback (Linux virtual camera)
    V4l2Loopback,
    /// Direct window output (default)
    Window,
}

impl std::fmt::Display for OutputBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OutputBackend::Ndi => write!(f, "NDI"),
            OutputBackend::PipeWire => write!(f, "PipeWire"),
            OutputBackend::Spout => write!(f, "Spout"),
            OutputBackend::V4l2Loopback => write!(f, "v4l2loopback"),
            OutputBackend::Window => write!(f, "Window"),
        }
    }
}

/// Trait for video output implementations
pub trait VideoOutput: Send {
    /// Get the backend type
    fn backend(&self) -> OutputBackend;

    /// Send a frame from OpenGL texture ID (GPU-based, preferred when supported)
    fn send_frame(&mut self, texture_id: u32, width: u32, height: u32) -> Result<(), VideoOutputError>;

    /// Send a frame from RGBA pixel data (CPU-based, universal fallback)
    fn send_frame_rgba(&mut self, pixels: &[u8], width: u32, height: u32) -> Result<(), VideoOutputError> {
        // Default implementation: not supported
        let _ = (pixels, width, height);
        Err(VideoOutputError::SendError("RGBA frame sending not supported by this backend".to_string()))
    }

    /// Check if output is active
    fn is_active(&self) -> bool;

    /// Get the output name/identifier
    fn name(&self) -> &str {
        "unnamed"
    }

    /// Enable or disable the output
    fn set_active(&mut self, active: bool) {
        let _ = active;
    }
}

/// Window-based video output (default)
pub struct WindowOutput {
    active: bool,
}

impl WindowOutput {
    pub fn new() -> Self {
        Self { active: true }
    }
}

impl Default for WindowOutput {
    fn default() -> Self {
        Self::new()
    }
}

impl VideoOutput for WindowOutput {
    fn backend(&self) -> OutputBackend {
        OutputBackend::Window
    }

    fn send_frame(&mut self, _texture_id: u32, _width: u32, _height: u32) -> Result<(), VideoOutputError> {
        // Window output is handled by the GL context directly
        Ok(())
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn name(&self) -> &str {
        "window"
    }

    fn set_active(&mut self, active: bool) {
        self.active = active;
    }
}

// Platform-specific re-exports (modules declared in mod.rs)
#[cfg(target_os = "linux")]
pub use super::v4l2::{V4l2Config, V4l2DeviceInfo, V4l2Output};

// Re-export for convenience
pub use OutputBackend as Backend;

/// List available video output devices for a backend
pub fn list_outputs(backend: OutputBackend) -> Vec<String> {
    match backend {
        #[cfg(target_os = "linux")]
        OutputBackend::V4l2Loopback => {
            V4l2Output::list_devices()
                .into_iter()
                .map(|d| format!("{}: {} ({})", d.path.display(), d.name, d.driver))
                .collect()
        }
        #[cfg(target_os = "windows")]
        OutputBackend::Spout => {
            // Spout senders are named, no enumeration needed for sending
            // Return a single "Spout" option
            if super::spout::SpoutOutput::is_available() {
                vec!["Spout:OpenDrop".to_string()]
            } else {
                vec![]
            }
        }
        OutputBackend::Ndi => {
            // NDI senders are named, check if runtime available
            if super::ndi::NdiOutput::is_available() {
                vec!["NDI:OpenDrop".to_string()]
            } else {
                vec![]
            }
        }
        OutputBackend::Window => vec!["Default Window".to_string()],
        _ => vec![],
    }
}

/// Check if a backend is available on this platform
pub fn is_backend_available(backend: OutputBackend) -> bool {
    match backend {
        OutputBackend::Window => true,
        #[cfg(target_os = "linux")]
        OutputBackend::V4l2Loopback => {
            // Check if any v4l2loopback devices exist
            !V4l2Output::list_devices().is_empty()
        }
        #[cfg(target_os = "windows")]
        OutputBackend::Spout => {
            super::spout::SpoutOutput::is_available()
        }
        OutputBackend::Ndi => {
            super::ndi::NdiOutput::is_available()
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_backend_display() {
        assert_eq!(format!("{}", OutputBackend::Ndi), "NDI");
        assert_eq!(format!("{}", OutputBackend::PipeWire), "PipeWire");
        assert_eq!(format!("{}", OutputBackend::Spout), "Spout");
        assert_eq!(format!("{}", OutputBackend::V4l2Loopback), "v4l2loopback");
        assert_eq!(format!("{}", OutputBackend::Window), "Window");
    }

    #[test]
    fn test_window_output_creation() {
        let output = WindowOutput::new();
        assert!(output.is_active());
        assert_eq!(output.backend(), OutputBackend::Window);
        assert_eq!(output.name(), "window");
    }

    #[test]
    fn test_window_output_set_active() {
        let mut output = WindowOutput::new();
        assert!(output.is_active());

        output.set_active(false);
        assert!(!output.is_active());

        output.set_active(true);
        assert!(output.is_active());
    }

    #[test]
    fn test_window_output_send_frame() {
        let mut output = WindowOutput::new();
        // Window output always succeeds (it's handled by GL context)
        assert!(output.send_frame(0, 1920, 1080).is_ok());
    }

    #[test]
    fn test_window_backend_always_available() {
        assert!(is_backend_available(OutputBackend::Window));
    }

    #[test]
    fn test_list_window_outputs() {
        let outputs = list_outputs(OutputBackend::Window);
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0], "Default Window");
    }

    #[test]
    fn test_ndi_availability() {
        // NDI availability depends on runtime installation
        // Just verify the check runs without panic
        let _available = is_backend_available(OutputBackend::Ndi);
    }

    #[test]
    fn test_pipewire_not_available() {
        // PipeWire video is not implemented yet
        assert!(!is_backend_available(OutputBackend::PipeWire));
    }
}
