//! Native port of the web app's `src/lib/engine/keymap.ts:5-16` (`DEFAULT_KEYMAP`),
//! plus runtime remapping + persistence, transposing the MIDI-learn pattern,
//! `AppState::midi_learning`/`ui::midi`, to the keyboard: see `main.rs`'s
//! `WindowEvent::KeyboardInput` handler for the commit side, `ui::keymap` for
//! the panel.
//!
//! ## Wire-format decision
//! `winit::keyboard::Key` derives `Serialize`/`Deserialize` itself, but only
//! behind its own `serde` Cargo feature (off by default upstream), enabled
//! here via `app/Cargo.toml`. That alone doesn't make `HashMap<Key,
//! CommandId>` usable as a `UiConfig` field though: `Key`'s derived impl
//! serializes through `serialize_newtype_variant` (e.g. `Key::Named(NamedKey
//! ::Tab)` -> `{"Named":"Tab"}`), and `serde_json`'s map-*key* serializer
//! only accepts primitives/unit variants in key position: a struct-shaped
//! value there fails at serialize time with "key must be a string". So
//! `UiConfig::keymap` (`config.rs`) stores `HashMap<String, String>`: each
//! `Key` is serialized to its own JSON string via `key_to_wire`/
//! `key_from_wire` below (using `Key`'s real derived impl, not a hand-rolled
//! reimplementation of its ~130-variant `NamedKey` enum) and used as the
//! *value* half of an outer string-keyed map, never the key half.
//! `CommandId` follows the pre-existing `command_names` convention
//! (`io/src/midi/mapping.rs`) rather than gaining its own `Serialize` impl:
//! wire name via `opendrop_io::command_names::{command_id_name,
//! parse_command_id}`.

use std::collections::HashMap;

use opendrop_core::commands::CommandId;
use opendrop_io::command_names::{command_id_name, parse_command_id};
use winit::keyboard::{Key, NamedKey};

pub fn default_keymap() -> HashMap<Key, CommandId> {
    let mut m = HashMap::new();
    m.insert(Key::Named(NamedKey::ArrowLeft), CommandId::CrossfaderLeft);
    m.insert(Key::Named(NamedKey::ArrowRight), CommandId::CrossfaderRight);
    m.insert(Key::Named(NamedKey::Tab), CommandId::DeckSwitch);
    m.insert(Key::Character("[".into()), CommandId::PresetPrevActive);
    m.insert(Key::Character("]".into()), CommandId::PresetNextActive);
    m.insert(Key::Named(NamedKey::Space), CommandId::PlaylistToggleActive);
    m.insert(Key::Character("n".into()), CommandId::PlaylistNextActive);
    m.insert(Key::Character("N".into()), CommandId::PlaylistNextActive);
    m.insert(Key::Character("p".into()), CommandId::PlaylistPrevActive);
    m.insert(Key::Character("P".into()), CommandId::PlaylistPrevActive);
    m
}

/// Human-readable label for a key, e.g. the Keymap panel's "assigned key"
/// column: equivalent of the web reference's `formatKey()`. Display only:
/// unlike `key_to_wire`, this doesn't need to round-trip (`Key::Character`
/// case isn't case-normalized, so "n" and "N", both bound in
/// `default_keymap` above, still read as distinct keys).
pub fn format_key(key: &Key) -> String {
    match key {
        Key::Named(named) => format!("{named:?}"),
        Key::Character(s) => s.to_string(),
        Key::Unidentified(_) => "Unknown Key".to_string(),
        Key::Dead(_) => "Dead Key".to_string(),
    }
}

/// `Key` -> its own JSON string, via `Key`'s real `Serialize` impl (see this
/// module's doc comment); round-trips exactly, unlike `format_key`.
fn key_to_wire(key: &Key) -> String {
    serde_json::to_string(key).unwrap_or_default()
}

/// The inverse of `key_to_wire`. `None` for a malformed/foreign string.
fn key_from_wire(s: &str) -> Option<Key> {
    serde_json::from_str(s).ok()
}

/// `AppState::keymap` -> `UiConfig::keymap`'s on-disk shape.
pub fn keymap_to_wire(keymap: &HashMap<Key, CommandId>) -> HashMap<String, String> {
    keymap.iter().map(|(key, &id)| (key_to_wire(key), command_id_name(id).to_string())).collect()
}

/// The inverse of `keymap_to_wire`. An entry whose key or command name
/// doesn't parse (a stale/foreign `ui.json`) is skipped rather than failing
/// the whole load: same best-effort philosophy as `config::config_from_json`.
pub fn keymap_from_wire(wire: &HashMap<String, String>) -> HashMap<Key, CommandId> {
    wire.iter().filter_map(|(k, v)| Some((key_from_wire(k)?, parse_command_id(v)?))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_keymap_has_ten_bindings() {
        assert_eq!(default_keymap().len(), 10);
    }

    #[test]
    fn format_key_named_and_character() {
        assert_eq!(format_key(&Key::Named(NamedKey::ArrowLeft)), "ArrowLeft");
        assert_eq!(format_key(&Key::Character("[".into())), "[");
        assert_eq!(format_key(&Key::Character("n".into())), "n");
        assert_eq!(format_key(&Key::Character("N".into())), "N");
    }

    #[test]
    fn wire_round_trip_preserves_every_binding() {
        let keymap = default_keymap();
        let wire = keymap_to_wire(&keymap);
        let restored = keymap_from_wire(&wire);
        assert_eq!(restored, keymap);
    }

    #[test]
    fn empty_wire_round_trips_to_empty_map() {
        assert!(keymap_from_wire(&keymap_to_wire(&HashMap::new())).is_empty());
    }

    #[test]
    fn keymap_from_wire_skips_unparseable_entries() {
        let mut wire = HashMap::new();
        wire.insert("not json".to_string(), "crossfader".to_string());
        wire.insert(key_to_wire(&Key::Character("q".into())), "not-a-command".to_string());
        assert!(keymap_from_wire(&wire).is_empty());
    }

    #[test]
    fn keymap_from_wire_keeps_valid_entries_alongside_bad_ones() {
        let mut wire = HashMap::new();
        wire.insert("not json".to_string(), "crossfader".to_string());
        wire.insert(key_to_wire(&Key::Named(NamedKey::Escape)), "deck-switch".to_string());
        let restored = keymap_from_wire(&wire);
        assert_eq!(restored.get(&Key::Named(NamedKey::Escape)), Some(&CommandId::DeckSwitch));
        assert_eq!(restored.len(), 1);
    }
}
