//! Spout video output for Windows
//!
//! Shares frames via Spout for capture by OBS, Resolume, etc.
//!
//! ## Setup (Windows only)
//!
//! 1. Download SpoutLibrary.dll from the official Spout2 releases:
//!    <https://github.com/leadedge/Spout2/releases>
//!
//! 2. Place `SpoutLibrary.dll` (64-bit) in one of these locations:
//!    - Same folder as opendrop.exe
//!    - System PATH (e.g., C:\Windows\System32)
//!
//! 3. In OBS/Resolume: Add a "Spout2 Capture" source and select "OpenDrop"
//!
//! ## Implementation
//!
//! Uses SpoutLibrary.dll with COM-like vtable interface.
//! The vtable structure follows SPOUTLIBRARY from SpoutLibrary.h:
//! <https://github.com/leadedge/Spout2/blob/master/SPOUTSDK/SpoutLibrary/SpoutLibrary.h>

use super::output::{OutputBackend, VideoOutput, VideoOutputError};

#[cfg(target_os = "windows")]
use std::ffi::CString;

/// Spout output configuration
#[derive(Debug, Clone)]
pub struct SpoutConfig {
    /// Sender name (visible in receiving apps like OBS)
    pub sender_name: String,
    /// Output width
    pub width: u32,
    /// Output height
    pub height: u32,
}

impl Default for SpoutConfig {
    fn default() -> Self {
        Self {
            sender_name: "OpenDrop".to_string(),
            width: 1920,
            height: 1080,
        }
    }
}

/// Information about a Spout sender
#[derive(Debug, Clone)]
pub struct SpoutSenderInfo {
    pub name: String,
    pub width: u32,
    pub height: u32,
}

// FFI types for SpoutLibrary
#[cfg(target_os = "windows")]
mod ffi {
    use std::ffi::c_void;

    /// Opaque handle to Spout instance (pointer to C++ object)
    pub type SpoutHandle = *mut c_void;

    // OpenGL types
    pub type GLuint = u32;
    pub type GLenum = u32;
    pub const GL_RGBA: GLenum = 0x1908;
    pub const GL_TEXTURE_2D: GLenum = 0x0DE1;

    /// Factory function exported by SpoutLibrary.dll
    pub type GetSpoutFn = unsafe extern "system" fn() -> SpoutHandle;

    /// SPOUTLIBRARY vtable - matches declaration order in SpoutLibrary.h
    ///
    /// The vtable indices (0-based) for key methods:
    /// - 0: SetSenderName
    /// - 1: SetSenderFormat
    /// - 2: ReleaseSender
    /// - 3: SendFbo
    /// - 4: SendTexture
    /// - 5: SendImage
    /// - 6: IsInitialized
    /// - 7: GetName
    /// - 8: GetWidth
    /// - 9: GetHeight
    /// - 10: GetFps
    /// - 11: GetFrame
    /// - 12: GetHandle
    /// - 13: GetCPU
    /// - 14: GetGLDX
    ///
    /// Full list (146 methods) at:
    /// https://github.com/leadedge/Spout2/blob/master/SPOUTSDK/SpoutLibrary/SpoutLibrary.h

    // Function pointer types matching __stdcall calling convention
    pub type SetSenderNameFn = unsafe extern "system" fn(*const i8);
    pub type SetSenderFormatFn = unsafe extern "system" fn(u32);
    pub type ReleaseSenderFn = unsafe extern "system" fn(u32);
    pub type SendFboFn = unsafe extern "system" fn(GLuint, u32, u32, bool) -> bool;
    pub type SendTextureFn = unsafe extern "system" fn(GLuint, GLuint, u32, u32, bool, GLuint) -> bool;
    pub type SendImageFn = unsafe extern "system" fn(*const u8, u32, u32, GLenum, bool) -> bool;
    pub type IsInitializedFn = unsafe extern "system" fn() -> bool;
    pub type GetNameFn = unsafe extern "system" fn() -> *const i8;
    pub type GetWidthFn = unsafe extern "system" fn() -> u32;
    pub type GetHeightFn = unsafe extern "system" fn() -> u32;
    pub type ReleaseFn = unsafe extern "system" fn();

    /// Vtable indices for SPOUTLIBRARY interface
    pub const VTABLE_SET_SENDER_NAME: usize = 0;
    pub const VTABLE_SET_SENDER_FORMAT: usize = 1;
    pub const VTABLE_RELEASE_SENDER: usize = 2;
    pub const VTABLE_SEND_FBO: usize = 3;
    pub const VTABLE_SEND_TEXTURE: usize = 4;
    pub const VTABLE_SEND_IMAGE: usize = 5;
    pub const VTABLE_IS_INITIALIZED: usize = 6;
    pub const VTABLE_GET_NAME: usize = 7;
    pub const VTABLE_GET_WIDTH: usize = 8;
    pub const VTABLE_GET_HEIGHT: usize = 9;
    // ... many more methods, we only use what we need
    pub const VTABLE_RELEASE: usize = 145; // Last method: Release()
}

#[cfg(target_os = "windows")]
use libloading::Library;

#[cfg(target_os = "windows")]
use std::ffi::c_void;

/// Spout video output using SpoutLibrary.dll
#[cfg(target_os = "windows")]
pub struct SpoutOutput {
    /// Handle to loaded SpoutLibrary.dll
    _library: Library,
    /// Spout instance handle (C++ object pointer)
    handle: ffi::SpoutHandle,
    /// Pointer to vtable
    vtable: *const *const c_void,
    /// Sender name
    name: String,
    /// Sender name as C string (keep alive)
    _sender_name_cstr: CString,
    /// Output dimensions
    width: u32,
    height: u32,
    /// Whether output is active
    active: bool,
    /// Whether sender is initialized (first frame sent)
    initialized: bool,
}

// SpoutOutput is Send because:
// - Library handle is thread-safe for our use case
// - The Spout handle is only used from one thread (renderer)
// - All vtable calls are synchronized by Rust's ownership
#[cfg(target_os = "windows")]
unsafe impl Send for SpoutOutput {}

#[cfg(target_os = "windows")]
impl SpoutOutput {
    /// Create a new Spout output
    pub fn new(config: SpoutConfig) -> Result<Self, VideoOutputError> {
        // Try to load SpoutLibrary.dll from multiple locations
        let library = unsafe {
            Library::new("SpoutLibrary.dll")
                .or_else(|_| Library::new("./SpoutLibrary.dll"))
                .or_else(|_| Library::new("bin/SpoutLibrary.dll"))
                .map_err(|e| {
                    VideoOutputError::InitError(format!(
                        "Failed to load SpoutLibrary.dll: {}. \n\
                        Download from https://github.com/leadedge/Spout2/releases \n\
                        and place in the application folder.",
                        e
                    ))
                })?
        };

        // Get the GetSpout factory function
        let get_spout: libloading::Symbol<ffi::GetSpoutFn> = unsafe {
            library.get(b"GetSpout\0").map_err(|e| {
                VideoOutputError::InitError(format!(
                    "Failed to find GetSpout function in SpoutLibrary.dll: {}",
                    e
                ))
            })?
        };

        // Create Spout instance
        let handle = unsafe { get_spout() };
        if handle.is_null() {
            return Err(VideoOutputError::InitError(
                "GetSpout() returned null handle".to_string()
            ));
        }

        // Get vtable pointer (first pointer in the C++ object)
        let vtable = unsafe { *(handle as *const *const *const c_void) };
        if vtable.is_null() {
            return Err(VideoOutputError::InitError(
                "Spout vtable is null".to_string()
            ));
        }

        // Create sender name C string (must be kept alive)
        let sender_name_cstr = CString::new(config.sender_name.as_str())
            .map_err(|_| VideoOutputError::InitError("Invalid sender name (contains null byte)".to_string()))?;

        // Call SetSenderName via vtable
        unsafe {
            let set_sender_name_ptr = *vtable.add(ffi::VTABLE_SET_SENDER_NAME);
            let set_sender_name: ffi::SetSenderNameFn = std::mem::transmute(set_sender_name_ptr);
            set_sender_name(sender_name_cstr.as_ptr());
        }

        tracing::info!(
            "Spout output created: {} ({}x{})",
            config.sender_name,
            config.width,
            config.height
        );

        Ok(Self {
            _library: library,
            handle,
            vtable,
            name: format!("spout:{}", config.sender_name),
            _sender_name_cstr: sender_name_cstr,
            width: config.width,
            height: config.height,
            active: true,
            initialized: false,
        })
    }

    /// List available Spout senders (for receiving)
    pub fn list_senders() -> Vec<SpoutSenderInfo> {
        // Senders are discovered dynamically by receiving apps
        // OpenDrop is a sender, not a receiver
        Vec::new()
    }

    /// Check if Spout is available on this system
    pub fn is_available() -> bool {
        unsafe {
            Library::new("SpoutLibrary.dll")
                .or_else(|_| Library::new("./SpoutLibrary.dll"))
                .or_else(|_| Library::new("bin/SpoutLibrary.dll"))
                .is_ok()
        }
    }

    /// Call SendTexture via vtable (GPU-accelerated)
    unsafe fn call_send_texture(&self, texture_id: u32, width: u32, height: u32) -> bool {
        let send_texture_ptr = *self.vtable.add(ffi::VTABLE_SEND_TEXTURE);
        let send_texture: ffi::SendTextureFn = std::mem::transmute(send_texture_ptr);
        send_texture(
            texture_id,
            ffi::GL_TEXTURE_2D,
            width,
            height,
            true,  // bInvert - flip vertically for OpenGL
            0,     // HostFBO - 0 = use default FBO
        )
    }

    /// Call SendImage via vtable (CPU fallback)
    unsafe fn call_send_image(&self, pixels: *const u8, width: u32, height: u32) -> bool {
        let send_image_ptr = *self.vtable.add(ffi::VTABLE_SEND_IMAGE);
        let send_image: ffi::SendImageFn = std::mem::transmute(send_image_ptr);
        send_image(
            pixels,
            width,
            height,
            ffi::GL_RGBA,
            false,  // bInvert - already in correct orientation
        )
    }

    /// Call ReleaseSender via vtable
    unsafe fn call_release_sender(&self) {
        let release_sender_ptr = *self.vtable.add(ffi::VTABLE_RELEASE_SENDER);
        let release_sender: ffi::ReleaseSenderFn = std::mem::transmute(release_sender_ptr);
        release_sender(0); // dwMsec = 0, no wait
    }

    /// Call Release via vtable (cleanup instance)
    unsafe fn call_release(&self) {
        let release_ptr = *self.vtable.add(ffi::VTABLE_RELEASE);
        let release: ffi::ReleaseFn = std::mem::transmute(release_ptr);
        release();
    }
}

#[cfg(target_os = "windows")]
impl VideoOutput for SpoutOutput {
    fn backend(&self) -> OutputBackend {
        OutputBackend::Spout
    }

    fn send_frame(&mut self, texture_id: u32, width: u32, height: u32) -> Result<(), VideoOutputError> {
        if !self.active {
            return Ok(());
        }

        // Update dimensions if changed
        if width != self.width || height != self.height {
            tracing::debug!(
                "Spout resolution changed: {}x{} -> {}x{}",
                self.width, self.height, width, height
            );
            self.width = width;
            self.height = height;
        }

        // Call SendTexture via vtable (GPU-accelerated texture sharing)
        let success = unsafe { self.call_send_texture(texture_id, width, height) };

        if success {
            if !self.initialized {
                tracing::info!("Spout sender initialized: {}", self.name);
                self.initialized = true;
            }
            Ok(())
        } else {
            Err(VideoOutputError::SendError(
                "SendTexture failed - is the OpenGL context current?".to_string()
            ))
        }
    }

    fn send_frame_rgba(&mut self, pixels: &[u8], width: u32, height: u32) -> Result<(), VideoOutputError> {
        if !self.active {
            return Ok(());
        }

        // Validate buffer size
        let expected_size = (width * height * 4) as usize;
        if pixels.len() != expected_size {
            return Err(VideoOutputError::SendError(format!(
                "Invalid pixel buffer size: got {}, expected {}",
                pixels.len(), expected_size
            )));
        }

        // Update dimensions if changed
        if width != self.width || height != self.height {
            self.width = width;
            self.height = height;
        }

        // Call SendImage via vtable (CPU fallback)
        let success = unsafe { self.call_send_image(pixels.as_ptr(), width, height) };

        if success {
            if !self.initialized {
                tracing::info!("Spout sender initialized (CPU mode): {}", self.name);
                self.initialized = true;
            }
            Ok(())
        } else {
            Err(VideoOutputError::SendError(
                "SendImage failed".to_string()
            ))
        }
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn set_active(&mut self, active: bool) {
        if !active && self.active && self.initialized {
            // Release sender when deactivating
            unsafe { self.call_release_sender() };
            self.initialized = false;
            tracing::debug!("Spout sender released: {}", self.name);
        }
        self.active = active;
    }
}

#[cfg(target_os = "windows")]
impl Drop for SpoutOutput {
    fn drop(&mut self) {
        // Release sender and cleanup
        if self.initialized {
            unsafe { self.call_release_sender() };
        }
        unsafe { self.call_release() };
        tracing::debug!("Spout output dropped: {}", self.name);
    }
}

// Non-Windows stub implementation
#[cfg(not(target_os = "windows"))]
pub struct SpoutOutput {
    _private: (),
}

#[cfg(not(target_os = "windows"))]
impl SpoutOutput {
    pub fn new(_config: SpoutConfig) -> Result<Self, VideoOutputError> {
        Err(VideoOutputError::NotSupported)
    }

    pub fn list_senders() -> Vec<SpoutSenderInfo> {
        Vec::new()
    }

    pub fn is_available() -> bool {
        false
    }
}

#[cfg(not(target_os = "windows"))]
impl VideoOutput for SpoutOutput {
    fn backend(&self) -> OutputBackend {
        OutputBackend::Spout
    }

    fn send_frame(&mut self, _texture_id: u32, _width: u32, _height: u32) -> Result<(), VideoOutputError> {
        Err(VideoOutputError::NotSupported)
    }

    fn is_active(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spout_config_default() {
        let config = SpoutConfig::default();
        assert_eq!(config.sender_name, "OpenDrop");
        assert_eq!(config.width, 1920);
        assert_eq!(config.height, 1080);
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn test_spout_not_supported_on_non_windows() {
        let result = SpoutOutput::new(SpoutConfig::default());
        assert!(matches!(result, Err(VideoOutputError::NotSupported)));
    }
}
