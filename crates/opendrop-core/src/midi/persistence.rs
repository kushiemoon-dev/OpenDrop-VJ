//! MIDI mapping persistence
//!
//! Save and load MIDI mappings to/from JSON files.

use std::fs;
use std::path::{Path, PathBuf};

use super::mapping::{MidiAction, MidiMapping, MidiMessageType};

/// A preset containing a collection of MIDI mappings
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MidiPreset {
    /// Preset name
    pub name: String,
    /// Description of this preset
    #[serde(default)]
    pub description: String,
    /// Target controller (e.g., "Akai APC Mini", "Generic")
    #[serde(default)]
    pub controller: String,
    /// The mappings in this preset
    pub mappings: Vec<MidiMapping>,
}

impl MidiPreset {
    /// Create a new empty preset
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            controller: String::new(),
            mappings: Vec::new(),
        }
    }

    /// Save preset to a JSON file
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), std::io::Error> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        fs::write(path, json)
    }

    /// Load preset from a JSON file
    pub fn load(path: impl AsRef<Path>) -> Result<Self, std::io::Error> {
        let json = fs::read_to_string(path)?;
        serde_json::from_str(&json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}

/// Get the default MIDI presets directory
pub fn presets_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("opendrop").join("midi"))
}

/// List available MIDI presets
pub fn list_presets() -> Vec<PathBuf> {
    let Some(dir) = presets_dir() else {
        return Vec::new();
    };

    if !dir.exists() {
        return Vec::new();
    }

    fs::read_dir(&dir)
        .ok()
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|ext| ext == "json"))
                .collect()
        })
        .unwrap_or_default()
}

/// Create a default preset for common DJ controllers
pub fn create_generic_dj_preset() -> MidiPreset {
    let mut preset = MidiPreset::new("Generic DJ Controller");
    preset.description = "Basic mapping for 2-deck DJ controllers".to_string();
    preset.controller = "Generic".to_string();

    // Crossfader (usually CC 0 or 1)
    preset.mappings.push(MidiMapping::new(
        "Crossfader",
        MidiMessageType::ControlChange {
            channel: 0,
            controller: 0,
        },
        MidiAction::CrossfaderPosition,
    ));

    // Deck volumes (usually CC 7 on different channels)
    for deck in 0..2 {
        preset.mappings.push(MidiMapping::new(
            format!("Deck {} Volume", deck + 1),
            MidiMessageType::ControlChange {
                channel: deck,
                controller: 7,
            },
            MidiAction::DeckVolume(deck),
        ));

        // Play buttons (Note On)
        preset.mappings.push(MidiMapping::new(
            format!("Deck {} Play", deck + 1),
            MidiMessageType::NoteOn {
                channel: deck,
                note: 36,
            },
            MidiAction::DeckToggle(deck),
        ));

        // Next preset
        preset.mappings.push(MidiMapping::new(
            format!("Deck {} Next Preset", deck + 1),
            MidiMessageType::NoteOn {
                channel: deck,
                note: 37,
            },
            MidiAction::NextPreset(deck),
        ));

        // Previous preset
        preset.mappings.push(MidiMapping::new(
            format!("Deck {} Prev Preset", deck + 1),
            MidiMessageType::NoteOn {
                channel: deck,
                note: 38,
            },
            MidiAction::PreviousPreset(deck),
        ));

        // Random preset
        preset.mappings.push(MidiMapping::new(
            format!("Deck {} Random", deck + 1),
            MidiMessageType::NoteOn {
                channel: deck,
                note: 39,
            },
            MidiAction::RandomPreset(deck),
        ));
    }

    preset
}

/// Create an Akai APC Mini preset
pub fn create_apc_mini_preset() -> MidiPreset {
    let mut preset = MidiPreset::new("Akai APC Mini");
    preset.description = "Mapping for Akai APC Mini controller".to_string();
    preset.controller = "Akai APC Mini".to_string();

    // Faders (CC 48-56)
    // Fader 1: Crossfader
    preset.mappings.push(MidiMapping::new(
        "Crossfader",
        MidiMessageType::ControlChange {
            channel: 0,
            controller: 48,
        },
        MidiAction::CrossfaderPosition,
    ));

    // Faders 2-5: Deck volumes
    for deck in 0u8..4 {
        preset.mappings.push(MidiMapping::new(
            format!("Deck {} Volume", deck + 1),
            MidiMessageType::ControlChange {
                channel: 0,
                controller: 49 + deck,
            },
            MidiAction::DeckVolume(deck),
        ));
    }

    // Bottom row buttons (notes 64-71): Deck controls
    // Buttons 0-3: Play/Stop
    for deck in 0u8..4 {
        preset.mappings.push(MidiMapping::new(
            format!("Deck {} Toggle", deck + 1),
            MidiMessageType::NoteOn {
                channel: 0,
                note: 64 + deck,
            },
            MidiAction::DeckToggle(deck),
        ));
    }

    // Row 2 (notes 56-63): Next preset
    for deck in 0u8..4 {
        preset.mappings.push(MidiMapping::new(
            format!("Deck {} Next", deck + 1),
            MidiMessageType::NoteOn {
                channel: 0,
                note: 56 + deck,
            },
            MidiAction::NextPreset(deck),
        ));
    }

    // Row 3 (notes 48-55): Previous preset
    for deck in 0u8..4 {
        preset.mappings.push(MidiMapping::new(
            format!("Deck {} Prev", deck + 1),
            MidiMessageType::NoteOn {
                channel: 0,
                note: 48 + deck,
            },
            MidiAction::PreviousPreset(deck),
        ));
    }

    // Row 4 (notes 40-47): Random preset
    for deck in 0u8..4 {
        preset.mappings.push(MidiMapping::new(
            format!("Deck {} Random", deck + 1),
            MidiMessageType::NoteOn {
                channel: 0,
                note: 40 + deck,
            },
            MidiAction::RandomPreset(deck),
        ));
    }

    preset
}

/// Create a Novation Launchpad preset
pub fn create_launchpad_preset() -> MidiPreset {
    let mut preset = MidiPreset::new("Novation Launchpad");
    preset.description = "Mapping for Novation Launchpad".to_string();
    preset.controller = "Novation Launchpad".to_string();

    // Launchpad uses notes 0-63 for the 8x8 grid
    // Bottom row (0-7): Deck controls
    for deck in 0u8..4 {
        // Play/Stop
        preset.mappings.push(MidiMapping::new(
            format!("Deck {} Toggle", deck + 1),
            MidiMessageType::NoteOn {
                channel: 0,
                note: deck,
            },
            MidiAction::DeckToggle(deck),
        ));

        // Next (second row)
        preset.mappings.push(MidiMapping::new(
            format!("Deck {} Next", deck + 1),
            MidiMessageType::NoteOn {
                channel: 0,
                note: 16 + deck,
            },
            MidiAction::NextPreset(deck),
        ));

        // Previous (third row)
        preset.mappings.push(MidiMapping::new(
            format!("Deck {} Prev", deck + 1),
            MidiMessageType::NoteOn {
                channel: 0,
                note: 32 + deck,
            },
            MidiAction::PreviousPreset(deck),
        ));

        // Random (fourth row)
        preset.mappings.push(MidiMapping::new(
            format!("Deck {} Random", deck + 1),
            MidiMessageType::NoteOn {
                channel: 0,
                note: 48 + deck,
            },
            MidiAction::RandomPreset(deck),
        ));
    }

    preset
}

/// Create a Korg nanoKONTROL2 preset
pub fn create_nanokontrol2_preset() -> MidiPreset {
    let mut preset = MidiPreset::new("Korg nanoKONTROL2");
    preset.description = "Mapping for Korg nanoKONTROL2".to_string();
    preset.controller = "Korg nanoKONTROL2".to_string();

    // nanoKONTROL2 has 8 channels with faders, knobs, and buttons
    // Faders: CC 0-7 (channel 0)
    // First 4 faders: Deck volumes
    for deck in 0u8..4 {
        preset.mappings.push(MidiMapping::new(
            format!("Deck {} Volume", deck + 1),
            MidiMessageType::ControlChange {
                channel: 0,
                controller: deck,
            },
            MidiAction::DeckVolume(deck),
        ));
    }

    // Fader 5: Crossfader
    preset.mappings.push(MidiMapping::new(
        "Crossfader",
        MidiMessageType::ControlChange {
            channel: 0,
            controller: 4,
        },
        MidiAction::CrossfaderPosition,
    ));

    // S buttons (solo): CC 32-39 - Play/Stop
    for deck in 0u8..4 {
        preset.mappings.push(MidiMapping::new(
            format!("Deck {} Toggle", deck + 1),
            MidiMessageType::ControlChange {
                channel: 0,
                controller: 32 + deck,
            },
            MidiAction::DeckToggle(deck),
        ));
    }

    // M buttons (mute): CC 48-55 - Next preset
    for deck in 0u8..4 {
        preset.mappings.push(MidiMapping::new(
            format!("Deck {} Next", deck + 1),
            MidiMessageType::ControlChange {
                channel: 0,
                controller: 48 + deck,
            },
            MidiAction::NextPreset(deck),
        ));
    }

    // R buttons (record): CC 64-71 - Random preset
    for deck in 0u8..4 {
        preset.mappings.push(MidiMapping::new(
            format!("Deck {} Random", deck + 1),
            MidiMessageType::ControlChange {
                channel: 0,
                controller: 64 + deck,
            },
            MidiAction::RandomPreset(deck),
        ));
    }

    // Knobs: CC 16-23 - Beat sensitivity
    for deck in 0u8..4 {
        preset.mappings.push(MidiMapping::new(
            format!("Deck {} Beat Sens", deck + 1),
            MidiMessageType::ControlChange {
                channel: 0,
                controller: 16 + deck,
            },
            MidiAction::DeckBeatSensitivity(deck),
        ));
    }

    preset
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_preset_new() {
        let preset = MidiPreset::new("Test Preset");
        assert_eq!(preset.name, "Test Preset");
        assert!(preset.mappings.is_empty());
    }

    #[test]
    fn test_preset_save_load() {
        let mut preset = MidiPreset::new("Test");
        preset.description = "A test preset".to_string();
        preset.mappings.push(MidiMapping::new(
            "Test Mapping",
            MidiMessageType::ControlChange {
                channel: 0,
                controller: 1,
            },
            MidiAction::CrossfaderPosition,
        ));

        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();

        // Save
        preset.save(path).unwrap();

        // Load
        let loaded = MidiPreset::load(path).unwrap();
        assert_eq!(loaded.name, "Test");
        assert_eq!(loaded.description, "A test preset");
        assert_eq!(loaded.mappings.len(), 1);
        assert_eq!(loaded.mappings[0].name, "Test Mapping");
    }

    #[test]
    fn test_generic_dj_preset() {
        let preset = create_generic_dj_preset();
        assert!(!preset.mappings.is_empty());
        assert!(preset.mappings.iter().any(|m| m.name == "Crossfader"));
    }

    #[test]
    fn test_apc_mini_preset() {
        let preset = create_apc_mini_preset();
        assert!(!preset.mappings.is_empty());
        assert_eq!(preset.controller, "Akai APC Mini");
    }

    #[test]
    fn test_launchpad_preset() {
        let preset = create_launchpad_preset();
        assert!(!preset.mappings.is_empty());
        assert_eq!(preset.controller, "Novation Launchpad");
    }

    #[test]
    fn test_nanokontrol2_preset() {
        let preset = create_nanokontrol2_preset();
        assert!(!preset.mappings.is_empty());
        assert_eq!(preset.controller, "Korg nanoKONTROL2");
    }
}
