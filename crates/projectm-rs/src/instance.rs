//! ProjectM instance wrapper

use std::cell::UnsafeCell;
use std::ffi::CString;
use std::marker::PhantomData;
use std::path::Path;
use std::ptr::NonNull;

use tracing::{debug, error};

use crate::{Channels, Error, Preset};

/// ProjectM visualization instance
///
/// This is the main interface for rendering Milkdrop visualizations.
/// Each instance manages its own OpenGL context and can render independently.
pub struct ProjectM {
    handle: NonNull<projectm_sys::projectm>,
    width: u32,
    height: u32,
    preset_path: Option<String>,
    // Prevent Send/Sync - ProjectM must stay on one thread
    _marker: PhantomData<UnsafeCell<()>>,
}

impl ProjectM {
    /// Create a new ProjectM instance
    ///
    /// # Arguments
    /// * `width` - Render width in pixels
    /// * `height` - Render height in pixels
    ///
    /// # Note
    /// An OpenGL context must be current when calling this function.
    pub fn new(width: u32, height: u32) -> Result<Self, Error> {
        debug!("Creating ProjectM instance {}x{}", width, height);

        let handle = unsafe { projectm_sys::projectm_create() };

        let handle = NonNull::new(handle).ok_or_else(|| {
            error!("Failed to create projectM instance");
            Error::InitFailed("projectm_create returned null".to_string())
        })?;

        let mut instance = Self {
            handle,
            width,
            height,
            preset_path: None,
            _marker: PhantomData,
        };

        // Set initial window size
        instance.resize(width, height);

        debug!("ProjectM instance created successfully");
        Ok(instance)
    }

    /// Get the raw projectm handle (for advanced usage)
    pub fn raw_handle(&self) -> projectm_sys::projectm_handle {
        self.handle.as_ptr()
    }

    /// Get the render dimensions
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Resize the render target
    pub fn resize(&mut self, width: u32, height: u32) {
        debug!("Resizing ProjectM to {}x{}", width, height);
        self.width = width;
        self.height = height;
        unsafe {
            projectm_sys::projectm_set_window_size(
                self.handle.as_ptr(),
                width as usize,
                height as usize,
            );
        }
    }

    /// Load a preset from file
    pub fn load_preset<P: AsRef<Path>>(&mut self, path: P, smooth: bool) -> Result<(), Error> {
        let path = path.as_ref();
        debug!("Loading preset: {}", path.display());

        if !path.exists() {
            return Err(Error::PresetLoadFailed(format!(
                "File not found: {}",
                path.display()
            )));
        }

        let path_str = path.to_string_lossy();
        let c_path = CString::new(path_str.as_bytes()).map_err(|_| {
            Error::PresetLoadFailed("Invalid path encoding".to_string())
        })?;

        unsafe {
            projectm_sys::projectm_load_preset_file(
                self.handle.as_ptr(),
                c_path.as_ptr(),
                smooth,
            );
        }

        self.preset_path = Some(path_str.to_string());
        Ok(())
    }

    /// Load a preset object
    pub fn load_preset_obj(&mut self, preset: &Preset, smooth: bool) -> Result<(), Error> {
        self.load_preset(&preset.path, smooth)
    }

    /// Get the currently loaded preset path
    pub fn current_preset(&self) -> Option<&str> {
        self.preset_path.as_deref()
    }

    /// Add PCM audio data (mono)
    pub fn add_pcm_mono(&mut self, samples: &[f32]) {
        unsafe {
            projectm_sys::projectm_pcm_add_float(
                self.handle.as_ptr(),
                samples.as_ptr(),
                samples.len() as u32,
                projectm_sys::projectm_channels_PROJECTM_MONO,
            );
        }
    }

    /// Add PCM audio data (stereo interleaved)
    pub fn add_pcm_stereo(&mut self, samples: &[f32]) {
        unsafe {
            projectm_sys::projectm_pcm_add_float(
                self.handle.as_ptr(),
                samples.as_ptr(),
                (samples.len() / 2) as u32, // count is number of sample pairs
                projectm_sys::projectm_channels_PROJECTM_STEREO,
            );
        }
    }

    /// Add PCM audio data with specified channel configuration
    pub fn add_pcm(&mut self, samples: &[f32], channels: Channels) {
        match channels {
            Channels::Mono => self.add_pcm_mono(samples),
            Channels::Stereo => self.add_pcm_stereo(samples),
        }
    }

    /// Render a single frame
    ///
    /// # Note
    /// An OpenGL context must be current when calling this function.
    /// The output is rendered to the currently bound framebuffer.
    pub fn render_frame(&mut self) {
        unsafe {
            projectm_sys::projectm_opengl_render_frame(self.handle.as_ptr());
        }
    }

    /// Set the beat sensitivity (0.0 to 2.0, default 1.0)
    pub fn set_beat_sensitivity(&mut self, sensitivity: f32) {
        let sensitivity = sensitivity.clamp(0.0, 2.0);
        debug!("Setting beat sensitivity to {}", sensitivity);
        unsafe {
            projectm_sys::projectm_set_beat_sensitivity(self.handle.as_ptr(), sensitivity);
        }
    }

    /// Get the current beat sensitivity
    pub fn beat_sensitivity(&self) -> f32 {
        unsafe { projectm_sys::projectm_get_beat_sensitivity(self.handle.as_ptr()) }
    }

    /// Set the preset duration in seconds
    pub fn set_preset_duration(&mut self, seconds: f64) {
        debug!("Setting preset duration to {} seconds", seconds);
        unsafe {
            projectm_sys::projectm_set_preset_duration(self.handle.as_ptr(), seconds);
        }
    }

    /// Get the preset duration in seconds
    pub fn preset_duration(&self) -> f64 {
        unsafe { projectm_sys::projectm_get_preset_duration(self.handle.as_ptr()) }
    }

    /// Enable or disable hard cuts (instant preset transitions on beat)
    pub fn set_hard_cut_enabled(&mut self, enabled: bool) {
        debug!("Setting hard cut enabled: {}", enabled);
        unsafe {
            projectm_sys::projectm_set_hard_cut_enabled(self.handle.as_ptr(), enabled);
        }
    }

    /// Set the soft cut duration (transition time) in seconds
    pub fn set_soft_cut_duration(&mut self, seconds: f64) {
        debug!("Setting soft cut duration to {} seconds", seconds);
        unsafe {
            projectm_sys::projectm_set_soft_cut_duration(self.handle.as_ptr(), seconds);
        }
    }

    /// Lock the current preset (prevent automatic changes)
    pub fn set_preset_locked(&mut self, locked: bool) {
        debug!("Setting preset locked: {}", locked);
        unsafe {
            projectm_sys::projectm_set_preset_locked(self.handle.as_ptr(), locked);
        }
    }

    /// Check if preset is locked
    pub fn is_preset_locked(&self) -> bool {
        unsafe { projectm_sys::projectm_get_preset_locked(self.handle.as_ptr()) }
    }

    /// Set the mesh size (affects quality/performance)
    pub fn set_mesh_size(&mut self, width: usize, height: usize) {
        debug!("Setting mesh size to {}x{}", width, height);
        unsafe {
            projectm_sys::projectm_set_mesh_size(self.handle.as_ptr(), width, height);
        }
    }

    /// Set the target FPS
    pub fn set_fps(&mut self, fps: i32) {
        debug!("Setting target FPS to {}", fps);
        unsafe {
            projectm_sys::projectm_set_fps(self.handle.as_ptr(), fps);
        }
    }

    /// Get the current FPS setting
    pub fn fps(&self) -> i32 {
        unsafe { projectm_sys::projectm_get_fps(self.handle.as_ptr()) }
    }

    /// Enable or disable aspect correction
    pub fn set_aspect_correction(&mut self, enabled: bool) {
        unsafe {
            projectm_sys::projectm_set_aspect_correction(self.handle.as_ptr(), enabled);
        }
    }

    /// Get projectM version string
    pub fn version() -> String {
        unsafe {
            let ptr = projectm_sys::projectm_get_version_string();
            if ptr.is_null() {
                return "unknown".to_string();
            }
            let c_str = std::ffi::CStr::from_ptr(ptr);
            let version = c_str.to_string_lossy().to_string();
            // Note: The returned string should be freed, but projectM docs are unclear
            // For safety, we don't free it here
            version
        }
    }
}

impl Drop for ProjectM {
    fn drop(&mut self) {
        debug!("Destroying ProjectM instance");
        unsafe {
            projectm_sys::projectm_destroy(self.handle.as_ptr());
        }
    }
}
