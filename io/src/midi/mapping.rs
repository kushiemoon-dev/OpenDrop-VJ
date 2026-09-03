//! Mapping persistence: `HashMap<CommandId, MidiTriggerKey>` <-> JSON on
//! disk at `ProjectDirs::from("", "", "opendrop-native").config_dir()
//! /midi_mappings.json`.
//!
//! `CommandId` doesn't derive `Serialize`/`Deserialize` (and shouldn't grow
//! a custom impl just for this), so the on-disk shape is
//! `HashMap<String, MidiTriggerKey>` keyed by the same kebab-case wire name
//! `opendrop_io::command_names` already uses for OSC/remote-WS: converted
//! to/from `CommandId` at the (de)serialize boundary via
//! `command_id_name`/`parse_command_id`, per the brief's explicit steer
//! ("simpler, avoids a custom `Serialize` on `CommandId`").

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::command_names::{command_id_name, parse_command_id};

use super::types::{MidiMapping, MidiTriggerKey};

/// `directories::ProjectDirs::from("", "", "opendrop-native").config_dir()
/// /midi_mappings.json`, or `None` if the OS gives us no home/config
/// directory at all (headless/CI environment): the caller treats that the
/// same as "no mapping file yet" (start with an empty mapping, and skip
/// persisting future changes rather than panicking).
pub(crate) fn mapping_file_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "opendrop-native").map(|dirs| dirs.config_dir().join("midi_mappings.json"))
}

/// `MidiMapping` -> pretty-printed JSON string (human-editable on disk).
fn mapping_to_json(mapping: &MidiMapping) -> String {
    let named: HashMap<&'static str, &MidiTriggerKey> = mapping.iter().map(|(id, key)| (command_id_name(*id), key)).collect();
    serde_json::to_string_pretty(&named).unwrap_or_else(|_| "{}".to_string())
}

/// JSON string -> `MidiMapping`. Unknown command names (a mapping file from
/// a newer/older build) and malformed JSON both degrade to "skip the
/// offending entries" / "empty mapping" rather than erroring: a stale or
/// corrupt mapping file must never stop MIDI from connecting.
fn mapping_from_json(json: &str) -> MidiMapping {
    let named: HashMap<String, MidiTriggerKey> = serde_json::from_str(json).unwrap_or_default();
    named.into_iter().filter_map(|(name, key)| parse_command_id(&name).map(|id| (id, key))).collect()
}

/// Loads the mapping from `path`, or an empty mapping if the file doesn't
/// exist yet or can't be read/parsed (logged once, never a panic).
pub(crate) fn load_mapping(path: &Path) -> MidiMapping {
    match std::fs::read_to_string(path) {
        Ok(json) => mapping_from_json(&json),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => MidiMapping::new(),
        Err(e) => {
            eprintln!("[midi] failed to read mapping file {}: {e}. Starting with an empty mapping.", path.display());
            MidiMapping::new()
        }
    }
}

/// Writes `mapping` to `path` as JSON, creating the parent directory if
/// needed. Best-effort: a write failure is logged, never a panic; losing
/// the ability to persist a mapping shouldn't take down MIDI I/O.
pub(crate) fn save_mapping(path: Option<&Path>, mapping: &MidiMapping) {
    let Some(path) = path else { return };
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            eprintln!("[midi] failed to create config dir {}: {e}. Mapping not saved.", parent.display());
            return;
        }
    }
    if let Err(e) = std::fs::write(path, mapping_to_json(mapping)) {
        eprintln!("[midi] failed to write mapping file {}: {e}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi::types::TriggerKind;
    use opendrop_core::commands::CommandId;

    fn key(device_id: &str, kind: TriggerKind, channel: u8, number: u8) -> MidiTriggerKey {
        MidiTriggerKey { device_id: device_id.to_string(), kind, channel, number }
    }

    #[test]
    fn json_round_trip_preserves_every_entry() {
        let mut mapping = MidiMapping::new();
        mapping.insert(CommandId::Crossfader, key("Akai APC40", TriggerKind::Cc, 1, 7));
        mapping.insert(CommandId::DeckSwitch, key("Akai APC40", TriggerKind::Note, 1, 60));
        mapping.insert(CommandId::CrossfaderLeft, key("Akai APC40", TriggerKind::Pitchbend, 3, 0));

        let json = mapping_to_json(&mapping);
        let restored = mapping_from_json(&json);

        assert_eq!(restored, mapping);
    }

    #[test]
    fn empty_mapping_round_trips_to_empty_map() {
        let mapping = MidiMapping::new();
        assert_eq!(mapping_from_json(&mapping_to_json(&mapping)), mapping);
    }

    #[test]
    fn unknown_command_name_in_json_is_skipped_not_fatal() {
        let json = r#"{"not-a-real-command":{"device_id":"dev","kind":"Cc","channel":1,"number":1}}"#;
        assert_eq!(mapping_from_json(json), MidiMapping::new());
    }

    #[test]
    fn malformed_json_degrades_to_empty_mapping() {
        assert_eq!(mapping_from_json("not json at all"), MidiMapping::new());
    }

    #[test]
    fn load_mapping_missing_file_is_empty_not_an_error() {
        let path = std::env::temp_dir().join("opendrop-native-test-mapping-does-not-exist.json");
        let _ = std::fs::remove_file(&path);
        assert_eq!(load_mapping(&path), MidiMapping::new());
    }

    #[test]
    fn save_then_load_round_trips_through_the_real_filesystem() {
        let dir = std::env::temp_dir().join(format!("opendrop-native-test-{}", std::process::id()));
        let path = dir.join("midi_mappings.json");
        let mut mapping = MidiMapping::new();
        mapping.insert(CommandId::Crossfader, key("Akai APC40", TriggerKind::Cc, 1, 7));

        save_mapping(Some(&path), &mapping);
        let restored = load_mapping(&path);

        assert_eq!(restored, mapping);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_mapping_with_no_path_is_a_silent_no_op() {
        save_mapping(None, &MidiMapping::new()); // must not panic
    }
}
