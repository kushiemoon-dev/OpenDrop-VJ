//! Pure types shared by the MIDI mapping/persistence/thread code: the
//! learned-trigger key shape, the thread's public snapshot/control
//! vocabulary, and small pure helpers (trigger derivation, mapping lookup,
//! LED status byte) that don't need `midir` or a live device to test.

use std::collections::HashMap;

use opendrop_core::commands::CommandId;
use serde::{Deserialize, Serialize};

use super::message::MidiEvent;

/// The kind of MIDI trigger a mapping is bound to. Mirrors `ParsedTriggerKey`'s
/// `kind` field, OpenDrop-VJ `src/lib/engine/midi.ts:37-53`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TriggerKind {
    Cc,
    Note,
    Pitchbend,
}

/// A learned MIDI trigger: which device, what kind of message, on which
/// channel/number. `device_id` is the MIDI port *name* (not midir's opaque,
/// backend-dependent port id): see the "device_id = port name" judgment
/// call in the task report for why. `number` is unused (fixed at 0) for
/// `TriggerKind::Pitchbend`, which has no CC/note number of its own:
/// mirrors the JS `ParsedTriggerKey` union, which simply omits the field
/// for the `pb` variant; our flat struct shape (one shape for all three
/// kinds, as specified by the brief) keeps the field but ignores it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MidiTriggerKey {
    pub device_id: String,
    pub kind: TriggerKind,
    pub channel: u8,
    pub number: u8,
}

/// The learned CC/note/pitchbend -> command mapping, in memory.
pub type MidiMapping = HashMap<CommandId, MidiTriggerKey>;

/// One resolved dispatch: `(CommandId, value01)`. NOT filtered by
/// soft-takeover: that comparison against the live app value (`Show`'s
/// crossfader) happens entirely in `app`'s `about_to_wait` loop (Task 8),
/// since this thread has no access to `Show`. See the task report for the
/// full ruling.
pub type MidiDispatch = (CommandId, f64);

/// Continuous state published via `MidiHandle::latest()`: never blocks,
/// always the latest known value (mirrors `AudioSnapshot`).
///
/// `clock_beat_count` and `hotplug_epoch` are monotonically-increasing
/// counters, not booleans/events: `app` remembers the last value it saw and
/// diffs against the current one each frame to detect "N quarter-note beats
/// fired since I last checked" / "an output port (re)connected since I last
/// checked", without needing a second channel (see task report for why this
/// was chosen over a dedicated clock/hotplug channel).
pub struct MidiSnapshot {
    pub connected: bool,
    pub device_names: Vec<String>,
    pub clock_bpm: f64,
    pub clock_beat_count: u64,
    pub hotplug_epoch: u64,
}

impl MidiSnapshot {
    pub fn disconnected() -> Self {
        MidiSnapshot { connected: false, device_names: Vec::new(), clock_bpm: 0.0, clock_beat_count: 0, hotplug_epoch: 0 }
    }
}

/// Outward control messages sent to the MIDI thread. `PushLed` isn't in the
/// brief's illustrative code sketch but is required by the `push_led`
/// method the brief explicitly asks for: see task report.
pub enum MidiControl {
    Connect,
    Disconnect,
    SelectPort(String),
    StartLearn(CommandId),
    ClearMapping(CommandId),
    PushLed(CommandId, bool),
}

/// Derives the `(MidiTriggerKey, value01)` pair for a decoded MIDI event,
/// or `None` for `MidiEvent::Clock` (which never resolves to a trigger:
/// clock bytes are routed to `MidiClockSync` instead, never into the
/// mapping/learn/dispatch path, mirroring the JS engine's separate
/// `onClock` vs `onMessage` callbacks).
///
/// `value01` is the 0..1 normalized value: 14-bit CC over 0..16383,
/// pitchbend over 0..16383 (always 14-bit), 7-bit CC/note velocity over
/// 0..127: mirrors `midi-connection-actions.ts:82`
/// (`msg.is14bit ? msg.value / 16383 : msg.value / 127`).
pub(crate) fn trigger_key_and_value(device_id: &str, event: MidiEvent) -> Option<(MidiTriggerKey, f64)> {
    match event {
        MidiEvent::Clock => None,
        MidiEvent::Pitchbend { channel, value } => {
            Some((MidiTriggerKey { device_id: device_id.to_string(), kind: TriggerKind::Pitchbend, channel, number: 0 }, value as f64 / 16383.0))
        }
        MidiEvent::Cc { channel, number, value, is_14bit } => {
            let denom = if is_14bit { 16383.0 } else { 127.0 };
            Some((MidiTriggerKey { device_id: device_id.to_string(), kind: TriggerKind::Cc, channel, number }, value as f64 / denom))
        }
        MidiEvent::Note { channel, number, value, on: _ } => {
            Some((MidiTriggerKey { device_id: device_id.to_string(), kind: TriggerKind::Note, channel, number }, value as f64 / 127.0))
        }
    }
}

/// `true` for a note-off event (either `0x80` status or `0x90` with
/// velocity 0): these never dispatch and never get learned, mirroring
/// `if (msg.type === 'note_off') { return / break }` in
/// `midi-connection-actions.ts:61,74`.
pub(crate) fn is_note_off(event: &MidiEvent) -> bool {
    matches!(event, MidiEvent::Note { on: false, .. })
}

/// Linear scan for the `CommandId` currently mapped to `key`, if any. A
/// full scan (not a reverse index) is deliberate: at most ~223 commands,
/// called at most once per incoming MIDI message (not per frame), so the
/// simplicity of a single source-of-truth map outweighs the O(1) win of a
/// second, easily-desynced reverse index.
pub(crate) fn resolve_mapping(mapping: &MidiMapping, key: &MidiTriggerKey) -> Option<CommandId> {
    mapping.iter().find(|(_, k)| *k == key).map(|(id, _)| *id)
}

/// The raw MIDI status byte for LED on/off feedback, or `None` for
/// `TriggerKind::Pitchbend` (a continuous control with no on/off LED
/// concept: mirrors `if (!parsed || parsed.kind === 'pb') return` in
/// `MidiEngine.sendFeedback`, `midi.ts:114`).
pub(crate) fn led_status_byte(kind: TriggerKind, channel: u8) -> Option<u8> {
    let base = match kind {
        TriggerKind::Note => 0x90,
        TriggerKind::Cc => 0xb0,
        TriggerKind::Pitchbend => return None,
    };
    Some(base | (channel.wrapping_sub(1) & 0x0f))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(device_id: &str, kind: TriggerKind, channel: u8, number: u8) -> MidiTriggerKey {
        MidiTriggerKey { device_id: device_id.to_string(), kind, channel, number }
    }

    #[test]
    fn clock_event_has_no_trigger_key() {
        assert_eq!(trigger_key_and_value("dev", MidiEvent::Clock), None);
    }

    #[test]
    fn cc_7bit_value01_divides_by_127() {
        let (k, v) = trigger_key_and_value("dev", MidiEvent::Cc { channel: 1, number: 10, value: 127, is_14bit: false }).unwrap();
        assert_eq!(k, key("dev", TriggerKind::Cc, 1, 10));
        assert!((v - 1.0).abs() < 1e-9);
    }

    #[test]
    fn cc_14bit_value01_divides_by_16383() {
        let (_, v) = trigger_key_and_value("dev", MidiEvent::Cc { channel: 1, number: 5, value: 16383, is_14bit: true }).unwrap();
        assert!((v - 1.0).abs() < 1e-9);
    }

    #[test]
    fn pitchbend_value01_divides_by_16383_and_number_is_zero() {
        let (k, v) = trigger_key_and_value("dev", MidiEvent::Pitchbend { channel: 4, value: 8192 }).unwrap();
        assert_eq!(k, key("dev", TriggerKind::Pitchbend, 4, 0));
        assert!((v - 8192.0 / 16383.0).abs() < 1e-9);
    }

    #[test]
    fn note_on_value01_divides_by_127() {
        let (k, v) = trigger_key_and_value("dev", MidiEvent::Note { channel: 1, number: 60, value: 100, on: true }).unwrap();
        assert_eq!(k, key("dev", TriggerKind::Note, 1, 60));
        assert!((v - 100.0 / 127.0).abs() < 1e-9);
    }

    #[test]
    fn note_off_is_detected_regardless_of_status_byte_shape() {
        assert!(is_note_off(&MidiEvent::Note { channel: 1, number: 60, value: 0, on: false }));
        assert!(!is_note_off(&MidiEvent::Note { channel: 1, number: 60, value: 0, on: true }));
        assert!(!is_note_off(&MidiEvent::Cc { channel: 1, number: 1, value: 0, is_14bit: false }));
    }

    #[test]
    fn resolve_mapping_finds_the_command_bound_to_a_key() {
        let mut mapping = MidiMapping::new();
        mapping.insert(CommandId::Crossfader, key("dev", TriggerKind::Cc, 1, 7));
        mapping.insert(CommandId::DeckSwitch, key("dev", TriggerKind::Note, 1, 60));

        assert_eq!(resolve_mapping(&mapping, &key("dev", TriggerKind::Cc, 1, 7)), Some(CommandId::Crossfader));
        assert_eq!(resolve_mapping(&mapping, &key("dev", TriggerKind::Note, 1, 60)), Some(CommandId::DeckSwitch));
        assert_eq!(resolve_mapping(&mapping, &key("other-dev", TriggerKind::Cc, 1, 7)), None);
        assert_eq!(resolve_mapping(&mapping, &key("dev", TriggerKind::Cc, 1, 99)), None);
    }

    #[test]
    fn led_status_byte_note_and_cc_encode_channel_minus_one() {
        assert_eq!(led_status_byte(TriggerKind::Note, 1), Some(0x90));
        assert_eq!(led_status_byte(TriggerKind::Note, 16), Some(0x90 | 0x0f));
        assert_eq!(led_status_byte(TriggerKind::Cc, 1), Some(0xb0));
        assert_eq!(led_status_byte(TriggerKind::Pitchbend, 1), None);
    }
}
