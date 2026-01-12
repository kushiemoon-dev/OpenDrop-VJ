//! Video output module

pub mod output;

#[cfg(target_os = "linux")]
pub mod v4l2;

#[cfg(target_os = "windows")]
pub mod spout;

pub mod ndi;

pub use output::{VideoOutput, VideoOutputError, OutputBackend};

#[cfg(target_os = "linux")]
pub use v4l2::{V4l2Config, V4l2DeviceInfo, V4l2Output};

#[cfg(target_os = "windows")]
pub use spout::{SpoutConfig, SpoutOutput, SpoutSenderInfo};

pub use ndi::{NdiConfig, NdiOutput, NdiSenderInfo};
