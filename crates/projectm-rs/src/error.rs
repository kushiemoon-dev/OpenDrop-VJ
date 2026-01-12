//! Error types for projectm-rs

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Failed to create projectM instance")]
    CreationFailed,

    #[error("Initialization failed: {0}")]
    InitFailed(String),

    #[error("Failed to load preset: {0}")]
    PresetLoadFailed(String),

    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),

    #[error("OpenGL error: {0}")]
    OpenGLError(String),

    #[error("Library not available")]
    LibraryNotAvailable,
}
