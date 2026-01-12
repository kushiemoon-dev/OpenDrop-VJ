//! OpenDrop Core Library
//!
//! Core functionality for the OpenDrop VJ visualizer.

pub mod audio;
pub mod deck;
pub mod midi;
pub mod render;
pub mod video;

pub use deck::Deck;
pub use render::{RenderWindow, RenderConfig, RenderCommand, RenderEvent, RenderError};
