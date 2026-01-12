//! Audio capture and processing module

pub mod capture;
pub mod ring_buffer;

#[cfg(target_os = "linux")]
pub mod pipewire;

pub use capture::{AudioBackend, AudioCapture, AudioConfig, AudioEngine, AudioError, DeviceInfo};

#[cfg(target_os = "linux")]
pub use pipewire::{PipeWireCapture, PipeWireConfig, PipeWireSource};
