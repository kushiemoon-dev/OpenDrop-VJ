//! OpenGL rendering module for projectM visualization
//!
//! This module handles creating OpenGL windows and rendering projectM visualizations.

mod window;

pub use window::{RenderWindow, RenderConfig, RenderCommand, RenderEvent, RenderError};
