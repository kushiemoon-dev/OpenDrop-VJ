//! `Session` (`opendrop_core::session`) <-> JSON, for the file the app
//! restores on the next launch.
//!
//! The shareable half reuses `share_codec`'s wire types, so a field added to
//! the share format is carried here too. The rest (playlists, strobe, LFO
//! slots, deck presets) get their own mirrors, for the same reason
//! `share_codec` has them: `core` stays free of serde. An LFO target that no
//! longer maps to a known command is dropped on decode, not an error.

use serde::{Deserialize, Serialize};

use opendrop_core::lfo::{LfoShape, LfoSlot, LFO_SLOTS};
use opendrop_core::playlist::PlaylistMode;
use opendrop_core::session::{PlaylistState, Session};
use opendrop_core::strobe::StrobeState;

use crate::command_names::{command_id_name, parse_command_id};
use crate::share_codec::{SharedSetWire, SHARE_WIRE_VERSION};

/// Bumped when the file layout changes in a way an older reader cannot
/// handle; compared for equality only, like the share link version.
const SESSION_WIRE_VERSION: u8 = 1;

#[derive(Serialize, Deserialize)]
enum PlaylistModeWire {
    Sequential,
    Shuffle,
}

#[derive(Serialize, Deserialize)]
struct PlaylistWire {
    interval_sec: f64,
    mode: PlaylistModeWire,
    a_items: Vec<String>,
    b_items: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct StrobeWire {
    enabled: bool,
    rate: f64,
    intensity: f64,
    color: [f32; 3],
}

#[derive(Serialize, Deserialize)]
enum LfoShapeWire {
    Sine,
    Saw,
    Square,
    Sh,
}

#[derive(Serialize, Deserialize)]
struct LfoSlotWire {
    enabled: bool,
    shape: LfoShapeWire,
    rate: f64,
    offset: f64,
    center: f64,
    amount: f64,
    target: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct SessionWire {
    #[serde(default)]
    version: u8,
    shared: SharedSetWire,
    deck_presets: [String; 4],
    selected_slot: usize,
    lock_a: bool,
    lock_b: bool,
    playlists: PlaylistWire,
    strobe: StrobeWire,
    lfo_slots: [LfoSlotWire; LFO_SLOTS],
}

impl From<PlaylistMode> for PlaylistModeWire {
    fn from(m: PlaylistMode) -> Self {
        match m {
            PlaylistMode::Sequential => Self::Sequential,
            PlaylistMode::Shuffle => Self::Shuffle,
        }
    }
}

impl From<PlaylistModeWire> for PlaylistMode {
    fn from(m: PlaylistModeWire) -> Self {
        match m {
            PlaylistModeWire::Sequential => Self::Sequential,
            PlaylistModeWire::Shuffle => Self::Shuffle,
        }
    }
}

impl From<LfoShape> for LfoShapeWire {
    fn from(s: LfoShape) -> Self {
        match s {
            LfoShape::Sine => Self::Sine,
            LfoShape::Saw => Self::Saw,
            LfoShape::Square => Self::Square,
            LfoShape::Sh => Self::Sh,
        }
    }
}

impl From<LfoShapeWire> for LfoShape {
    fn from(s: LfoShapeWire) -> Self {
        match s {
            LfoShapeWire::Sine => Self::Sine,
            LfoShapeWire::Saw => Self::Saw,
            LfoShapeWire::Square => Self::Square,
            LfoShapeWire::Sh => Self::Sh,
        }
    }
}

impl From<&LfoSlot> for LfoSlotWire {
    fn from(s: &LfoSlot) -> Self {
        Self {
            enabled: s.enabled,
            shape: s.shape.into(),
            rate: s.rate,
            offset: s.offset,
            center: s.center,
            amount: s.amount,
            target: s.target.map(|id| command_id_name(id).to_string()),
        }
    }
}

impl From<LfoSlotWire> for LfoSlot {
    fn from(w: LfoSlotWire) -> Self {
        Self {
            enabled: w.enabled,
            shape: w.shape.into(),
            rate: w.rate,
            offset: w.offset,
            center: w.center,
            amount: w.amount,
            target: w.target.as_deref().and_then(parse_command_id),
        }
    }
}

pub fn encode_session(session: &Session) -> Result<String, String> {
    let wire = SessionWire {
        version: SESSION_WIRE_VERSION,
        shared: SharedSetWire::from(&session.shared),
        deck_presets: session.deck_presets.clone(),
        selected_slot: session.selected_slot,
        lock_a: session.lock_a,
        lock_b: session.lock_b,
        playlists: PlaylistWire {
            interval_sec: session.playlists.interval_sec,
            mode: session.playlists.mode.into(),
            a_items: session.playlists.a_items.clone(),
            b_items: session.playlists.b_items.clone(),
        },
        strobe: StrobeWire {
            enabled: session.strobe.enabled,
            rate: session.strobe.rate,
            intensity: session.strobe.intensity,
            color: session.strobe.color,
        },
        lfo_slots: std::array::from_fn(|i| LfoSlotWire::from(&session.lfo_slots[i])),
    };
    serde_json::to_string(&wire).map_err(|e| format!("serializing session: {e}"))
}

pub fn decode_session(json: &str) -> Result<Session, String> {
    let wire: SessionWire = serde_json::from_str(json).map_err(|e| format!("parsing session JSON: {e}"))?;
    if wire.version != SESSION_WIRE_VERSION || wire.shared.version != SHARE_WIRE_VERSION {
        return Err(format!(
            "unsupported session version {} (this build reads version {SESSION_WIRE_VERSION})",
            wire.version
        ));
    }
    Ok(Session {
        shared: wire.shared.into(),
        deck_presets: wire.deck_presets,
        selected_slot: wire.selected_slot,
        lock_a: wire.lock_a,
        lock_b: wire.lock_b,
        playlists: PlaylistState {
            interval_sec: wire.playlists.interval_sec,
            mode: wire.playlists.mode.into(),
            a_items: wire.playlists.a_items,
            b_items: wire.playlists.b_items,
        },
        strobe: StrobeState {
            enabled: wire.strobe.enabled,
            rate: wire.strobe.rate,
            intensity: wire.strobe.intensity,
            color: wire.strobe.color,
        },
        lfo_slots: wire.lfo_slots.map(Into::into),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use opendrop_core::commands::{CommandId, Deck};
    use opendrop_core::show::{DeckBus, Show};

    fn sample_session() -> Session {
        let mut show = Show::default();
        show.crossfader = 0.7;
        show.deck_bus = [DeckBus::A, DeckBus::B, DeckBus::B, DeckBus::Off];
        show.lock_a = true;
        show.strobe.rate = 3.0;
        show.lfo_engine.slots[2] = LfoSlot {
            enabled: true,
            shape: LfoShape::Square,
            rate: 0.5,
            offset: 0.1,
            center: 0.6,
            amount: 0.2,
            target: Some(CommandId::Crossfader),
        };
        show.playlists.add_to_playlist(Deck::B, "deck b item".to_string());
        show.capture_session(["p0".into(), "p1".into(), "p2".into(), String::new()], 2.0)
    }

    #[test]
    fn a_session_survives_a_json_round_trip() {
        let original = sample_session();

        let restored = decode_session(&encode_session(&original).unwrap()).unwrap();

        assert_eq!(restored.deck_presets, original.deck_presets);
        assert_eq!(restored.shared.crossfader, 0.7);
        assert_eq!(restored.shared.deck_bus, original.shared.deck_bus);
        assert_eq!(restored.shared.transition_time, 2.0);
        assert!(restored.lock_a && !restored.lock_b);
        assert_eq!(restored.strobe.rate, 3.0);
        assert_eq!(restored.lfo_slots[2], original.lfo_slots[2]);
        assert_eq!(restored.playlists.b_items, vec!["deck b item".to_string()]);
    }

    #[test]
    fn an_unknown_lfo_target_is_dropped_not_an_error() {
        let json = encode_session(&sample_session()).unwrap().replace("\"target\":\"crossfader\"", "\"target\":\"renamed-away\"");

        let restored = decode_session(&json).unwrap();

        assert_eq!(restored.lfo_slots[2].target, None);
        assert!(restored.lfo_slots[2].enabled);
    }

    #[test]
    fn a_version_mismatch_or_garbage_is_rejected() {
        assert!(decode_session("not json").is_err());
        let json = encode_session(&sample_session()).unwrap().replacen("\"version\":1", "\"version\":9", 1);
        assert!(decode_session(&json).err().is_some_and(|e| e.contains("unsupported session version")));
    }
}
