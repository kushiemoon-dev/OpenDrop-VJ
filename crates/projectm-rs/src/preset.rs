//! Preset handling for projectM

use std::path::{Path, PathBuf};

/// A Milkdrop preset
#[derive(Debug, Clone)]
pub struct Preset {
    /// Path to the preset file
    pub path: PathBuf,
    /// Preset name (from filename)
    pub name: String,
}

impl Preset {
    /// Create a new preset from a file path
    pub fn from_path<P: AsRef<Path>>(path: P) -> Self {
        let path = path.as_ref().to_path_buf();
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown")
            .to_string();

        Self { path, name }
    }

    /// Get the preset file extension
    pub fn extension(&self) -> Option<&str> {
        self.path.extension().and_then(|s| s.to_str())
    }

    /// Check if this is a valid Milkdrop preset
    pub fn is_valid(&self) -> bool {
        matches!(self.extension(), Some("milk") | Some("prjm"))
    }
}

/// Scan a directory for presets
pub fn scan_presets<P: AsRef<Path>>(dir: P) -> Vec<Preset> {
    let dir = dir.as_ref();

    if !dir.is_dir() {
        return Vec::new();
    }

    walkdir(dir)
        .into_iter()
        .filter(|p| p.is_valid())
        .collect()
}

fn walkdir<P: AsRef<Path>>(dir: P) -> Vec<Preset> {
    let mut presets = Vec::new();

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();

            if path.is_dir() {
                presets.extend(walkdir(&path));
            } else if let Some(ext) = path.extension() {
                if ext == "milk" || ext == "prjm" {
                    presets.push(Preset::from_path(&path));
                }
            }
        }
    }

    presets
}
