//! Safe Rust wrapper for libprojectM
//!
//! This crate provides a safe, idiomatic Rust API for the Milkdrop-compatible
//! projectM visualization library.

mod instance;
mod preset;
mod error;

pub use instance::ProjectM;
pub use preset::{Preset, scan_presets};
pub use error::Error;

/// Audio channel configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channels {
    Mono,
    Stereo,
}

impl Channels {
    pub fn count(&self) -> usize {
        match self {
            Channels::Mono => 1,
            Channels::Stereo => 2,
        }
    }
}
