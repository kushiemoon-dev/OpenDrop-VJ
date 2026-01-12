//! MIDI mapping types and structures
//!
//! Defines the mapping between MIDI messages and OpenDrop actions.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A MIDI mapping connects a MIDI message to an OpenDrop action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MidiMapping {
    /// Unique identifier for this mapping
    pub id: Uuid,
    /// Human-readable name for this mapping
    pub name: String,
    /// The MIDI message that triggers this action
    pub midi_message: MidiMessageType,
    /// The action to perform when triggered
    pub action: MidiAction,
    /// Optional value transformation
    #[serde(default)]
    pub value_transform: Option<ValueTransform>,
    /// Whether this mapping is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

impl MidiMapping {
    /// Create a new MIDI mapping
    pub fn new(name: impl Into<String>, midi_message: MidiMessageType, action: MidiAction) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            midi_message,
            action,
            value_transform: None,
            enabled: true,
        }
    }

    /// Check if this mapping matches a MIDI message
    pub fn matches(&self, channel: u8, message: &MidiMessage) -> bool {
        if !self.enabled {
            return false;
        }
        self.midi_message.matches(channel, message)
    }

    /// Apply value transformation if configured
    pub fn transform_value(&self, value: f32) -> f32 {
        match &self.value_transform {
            Some(transform) => transform.apply(value),
            None => value,
        }
    }
}

/// Types of MIDI messages that can be mapped
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MidiMessageType {
    /// Note On event
    NoteOn {
        channel: u8,
        note: u8,
    },
    /// Note Off event
    NoteOff {
        channel: u8,
        note: u8,
    },
    /// Control Change (CC) event
    ControlChange {
        channel: u8,
        controller: u8,
    },
    /// Pitch Bend event
    PitchBend {
        channel: u8,
    },
    /// Program Change event
    ProgramChange {
        channel: u8,
    },
    /// Any MIDI message on a specific channel (for learn mode)
    AnyOnChannel {
        channel: u8,
    },
}

impl MidiMessageType {
    /// Check if this type matches an incoming MIDI message
    pub fn matches(&self, channel: u8, message: &MidiMessage) -> bool {
        match (self, message) {
            (MidiMessageType::NoteOn { channel: c, note: n }, MidiMessage::NoteOn { note, .. }) => {
                *c == channel && *n == *note
            }
            (MidiMessageType::NoteOff { channel: c, note: n }, MidiMessage::NoteOff { note, .. }) => {
                *c == channel && *n == *note
            }
            (
                MidiMessageType::ControlChange { channel: c, controller: ctrl },
                MidiMessage::ControlChange { controller, .. },
            ) => *c == channel && *ctrl == *controller,
            (MidiMessageType::PitchBend { channel: c }, MidiMessage::PitchBend { .. }) => {
                *c == channel
            }
            (MidiMessageType::ProgramChange { channel: c }, MidiMessage::ProgramChange { .. }) => {
                *c == channel
            }
            (MidiMessageType::AnyOnChannel { channel: c }, _) => *c == channel,
            _ => false,
        }
    }
}

/// Parsed MIDI message
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MidiMessage {
    NoteOn { note: u8, velocity: u8 },
    NoteOff { note: u8, velocity: u8 },
    ControlChange { controller: u8, value: u8 },
    PitchBend { value: u16 },
    ProgramChange { program: u8 },
    Unknown,
}

impl MidiMessage {
    /// Parse raw MIDI bytes into a MidiMessage
    pub fn parse(data: &[u8]) -> (u8, Self) {
        if data.is_empty() {
            return (0, MidiMessage::Unknown);
        }

        let status = data[0];
        let channel = status & 0x0F;
        let message_type = status & 0xF0;

        let message = match message_type {
            0x90 if data.len() >= 3 => {
                if data[2] == 0 {
                    // Note On with velocity 0 is Note Off
                    MidiMessage::NoteOff {
                        note: data[1],
                        velocity: 0,
                    }
                } else {
                    MidiMessage::NoteOn {
                        note: data[1],
                        velocity: data[2],
                    }
                }
            }
            0x80 if data.len() >= 3 => MidiMessage::NoteOff {
                note: data[1],
                velocity: data[2],
            },
            0xB0 if data.len() >= 3 => MidiMessage::ControlChange {
                controller: data[1],
                value: data[2],
            },
            0xE0 if data.len() >= 3 => {
                let value = ((data[2] as u16) << 7) | (data[1] as u16);
                MidiMessage::PitchBend { value }
            }
            0xC0 if data.len() >= 2 => MidiMessage::ProgramChange { program: data[1] },
            _ => MidiMessage::Unknown,
        };

        (channel, message)
    }

    /// Get the value from this message (0.0-1.0 range)
    pub fn value(&self) -> f32 {
        match self {
            MidiMessage::NoteOn { velocity, .. } => *velocity as f32 / 127.0,
            MidiMessage::NoteOff { .. } => 0.0,
            MidiMessage::ControlChange { value, .. } => *value as f32 / 127.0,
            MidiMessage::PitchBend { value } => *value as f32 / 16383.0,
            MidiMessage::ProgramChange { .. } => 1.0,
            MidiMessage::Unknown => 0.0,
        }
    }
}

/// Actions that can be triggered by MIDI
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MidiAction {
    // Deck controls
    DeckStart(u8),
    DeckStop(u8),
    DeckToggle(u8),
    DeckVolume(u8),
    DeckBeatSensitivity(u8),

    // Preset controls
    NextPreset(u8),
    PreviousPreset(u8),
    RandomPreset(u8),
    LoadPresetByIndex { deck: u8, index: usize },

    // Playlist controls
    PlaylistNext(u8),
    PlaylistPrevious(u8),
    PlaylistToggleShuffle(u8),
    PlaylistToggleAutoCycle(u8),

    // Crossfader
    CrossfaderPosition,
    CrossfaderCurve,
    CrossfaderToggle,

    // Global
    MasterVolume,
    ToggleFullscreen(u8),

    // Video output
    VideoOutputToggle(u8),
}

impl MidiAction {
    /// Get the deck ID if this action targets a specific deck
    pub fn deck_id(&self) -> Option<u8> {
        match self {
            MidiAction::DeckStart(d)
            | MidiAction::DeckStop(d)
            | MidiAction::DeckToggle(d)
            | MidiAction::DeckVolume(d)
            | MidiAction::DeckBeatSensitivity(d)
            | MidiAction::NextPreset(d)
            | MidiAction::PreviousPreset(d)
            | MidiAction::RandomPreset(d)
            | MidiAction::PlaylistNext(d)
            | MidiAction::PlaylistPrevious(d)
            | MidiAction::PlaylistToggleShuffle(d)
            | MidiAction::PlaylistToggleAutoCycle(d)
            | MidiAction::ToggleFullscreen(d)
            | MidiAction::VideoOutputToggle(d) => Some(*d),
            MidiAction::LoadPresetByIndex { deck, .. } => Some(*deck),
            _ => None,
        }
    }

    /// Whether this action uses continuous values (vs trigger)
    pub fn is_continuous(&self) -> bool {
        matches!(
            self,
            MidiAction::DeckVolume(_)
                | MidiAction::DeckBeatSensitivity(_)
                | MidiAction::CrossfaderPosition
                | MidiAction::MasterVolume
        )
    }
}

/// Value transformation for continuous controls
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ValueTransform {
    /// Minimum output value
    #[serde(default)]
    pub min: f32,
    /// Maximum output value
    #[serde(default = "default_max")]
    pub max: f32,
    /// Curve type
    #[serde(default)]
    pub curve: TransformCurve,
    /// Invert the value
    #[serde(default)]
    pub invert: bool,
}

fn default_max() -> f32 {
    1.0
}

impl Default for ValueTransform {
    fn default() -> Self {
        Self {
            min: 0.0,
            max: 1.0,
            curve: TransformCurve::Linear,
            invert: false,
        }
    }
}

impl ValueTransform {
    /// Apply the transformation to a value
    pub fn apply(&self, value: f32) -> f32 {
        let value = if self.invert { 1.0 - value } else { value };

        let curved = match self.curve {
            TransformCurve::Linear => value,
            TransformCurve::Logarithmic => value.sqrt(),
            TransformCurve::Exponential => value * value,
        };

        self.min + curved * (self.max - self.min)
    }
}

/// Curve type for value transformation
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransformCurve {
    #[default]
    Linear,
    Logarithmic,
    Exponential,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_note_on() {
        let data = [0x90, 60, 100]; // Note On, channel 0, note 60, velocity 100
        let (channel, msg) = MidiMessage::parse(&data);
        assert_eq!(channel, 0);
        assert!(matches!(msg, MidiMessage::NoteOn { note: 60, velocity: 100 }));
    }

    #[test]
    fn test_parse_note_on_velocity_zero_is_note_off() {
        let data = [0x91, 64, 0]; // Note On, channel 1, note 64, velocity 0
        let (channel, msg) = MidiMessage::parse(&data);
        assert_eq!(channel, 1);
        assert!(matches!(msg, MidiMessage::NoteOff { note: 64, .. }));
    }

    #[test]
    fn test_parse_note_off() {
        let data = [0x82, 48, 64]; // Note Off, channel 2, note 48
        let (channel, msg) = MidiMessage::parse(&data);
        assert_eq!(channel, 2);
        assert!(matches!(msg, MidiMessage::NoteOff { note: 48, .. }));
    }

    #[test]
    fn test_parse_control_change() {
        let data = [0xB3, 7, 100]; // CC, channel 3, controller 7, value 100
        let (channel, msg) = MidiMessage::parse(&data);
        assert_eq!(channel, 3);
        assert!(matches!(msg, MidiMessage::ControlChange { controller: 7, value: 100 }));
    }

    #[test]
    fn test_parse_pitch_bend() {
        let data = [0xE0, 0, 64]; // Pitch bend, channel 0, center position
        let (channel, msg) = MidiMessage::parse(&data);
        assert_eq!(channel, 0);
        if let MidiMessage::PitchBend { value } = msg {
            assert_eq!(value, 8192); // Center value
        } else {
            panic!("Expected PitchBend");
        }
    }

    #[test]
    fn test_mapping_matches() {
        let mapping = MidiMapping::new(
            "Test",
            MidiMessageType::ControlChange { channel: 0, controller: 1 },
            MidiAction::CrossfaderPosition,
        );

        let msg = MidiMessage::ControlChange { controller: 1, value: 64 };
        assert!(mapping.matches(0, &msg));

        let msg2 = MidiMessage::ControlChange { controller: 2, value: 64 };
        assert!(!mapping.matches(0, &msg2));
    }

    #[test]
    fn test_value_transform() {
        let transform = ValueTransform {
            min: 0.0,
            max: 100.0,
            curve: TransformCurve::Linear,
            invert: false,
        };

        assert!((transform.apply(0.0) - 0.0).abs() < 0.001);
        assert!((transform.apply(0.5) - 50.0).abs() < 0.001);
        assert!((transform.apply(1.0) - 100.0).abs() < 0.001);
    }

    #[test]
    fn test_value_transform_invert() {
        let transform = ValueTransform {
            min: 0.0,
            max: 1.0,
            curve: TransformCurve::Linear,
            invert: true,
        };

        assert!((transform.apply(0.0) - 1.0).abs() < 0.001);
        assert!((transform.apply(1.0) - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_midi_message_value() {
        let note_on = MidiMessage::NoteOn { note: 60, velocity: 127 };
        assert!((note_on.value() - 1.0).abs() < 0.01);

        let cc = MidiMessage::ControlChange { controller: 1, value: 64 };
        assert!((cc.value() - 0.504).abs() < 0.01);

        let note_off = MidiMessage::NoteOff { note: 60, velocity: 64 };
        assert!((note_off.value() - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_action_deck_id() {
        assert_eq!(MidiAction::DeckStart(2).deck_id(), Some(2));
        assert_eq!(MidiAction::CrossfaderPosition.deck_id(), None);
        assert_eq!(MidiAction::MasterVolume.deck_id(), None);
    }

    #[test]
    fn test_action_is_continuous() {
        assert!(MidiAction::DeckVolume(0).is_continuous());
        assert!(MidiAction::CrossfaderPosition.is_continuous());
        assert!(!MidiAction::DeckStart(0).is_continuous());
        assert!(!MidiAction::NextPreset(0).is_continuous());
    }
}
