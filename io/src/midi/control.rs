//! Handling for each `MidiControl` variant, plus the small `midir`
//! port-enumeration/connect helpers they (and `handle.rs`'s hotplug poll)
//! share. Split out of `handle.rs` to keep that file under the ~400-line
//! convention: see the task report for why.

use std::sync::mpsc::Sender;

use opendrop_core::commands::CommandId;

use super::handle::{RawMidi, ThreadState};
use super::mapping::{self, mapping_file_path};
use super::message::MidiParser;
use super::clock_sync::MidiClockSync;
use super::types::{led_status_byte, MidiControl};

/// ALSA/CoreMIDI/WinMM client name midir registers under.
pub(super) const CLIENT_NAME: &str = "OpenDrop-Native";

pub(super) fn handle_control(ts: &mut ThreadState, ctrl: MidiControl, midi_tx: &Sender<RawMidi>) {
    match ctrl {
        MidiControl::Connect => handle_connect(ts),
        MidiControl::Disconnect => handle_disconnect(ts),
        MidiControl::SelectPort(name) => handle_select_port(ts, name, midi_tx),
        MidiControl::StartLearn(id) => ts.learning = Some(id),
        MidiControl::StopLearn => ts.learning = None,
        MidiControl::ClearMapping(id) => {
            ts.mapping.remove(&id);
            mapping::save_mapping(ts.mapping_path.as_deref(), &ts.mapping);
        }
        MidiControl::PushLed(id, on) => handle_push_led(ts, id, on),
    }
}

/// Initializes the MIDI backend and loads the persisted mapping from disk
/// ("loaded once at connect time" per the brief): never opens a port by
/// itself, that's `SelectPort`'s job. Whole-branch review Finding M7:
/// `ts.connected` (surfaced by the panel as "MIDI: connected") must NOT be
/// set here: this only probes the backend and lists ports, it doesn't open
/// one. `ts.connected` only goes `true` once `handle_select_port` actually
/// opens an input connection.
fn handle_connect(ts: &mut ThreadState) {
    ts.mapping_path = mapping_file_path();
    ts.mapping = ts.mapping_path.as_deref().map(mapping::load_mapping).unwrap_or_default();

    match midir::MidiInput::new(CLIENT_NAME) {
        Ok(input) => {
            ts.device_names = list_port_names(&input);
        }
        Err(_) => {
            eprintln!("[midi] MIDI backend unavailable: no controller I/O this session");
            ts.device_names = Vec::new();
        }
    }
}

fn handle_disconnect(ts: &mut ThreadState) {
    ts.input_conn = None; // Drop closes the connection.
    ts.output_conn = None;
    ts.connected = false;
    ts.device_names = Vec::new();
    ts.clock_bpm = 0.0;
    ts.learning = None;
    ts.parser = MidiParser::new();
    ts.clock = MidiClockSync::new();
    ts.last_clock_pulse_at = None;
    // clock_beat_count and hotplug_epoch are NOT reset: they're monotonic
    // counters `app` diffs against, and resetting them could go backward.
}

/// Closes any previously-open input/output connection and opens the named
/// input port (by *name*: the same string shown in `device_names`), plus
/// its name-matched output port for LED feedback if one exists. Never
/// panics: an unplugged/renamed port, or any `midir` connect failure, is
/// logged and leaves the thread in a neutral "nothing selected" state.
fn handle_select_port(ts: &mut ThreadState, name: String, midi_tx: &Sender<RawMidi>) {
    ts.input_conn = None;
    ts.output_conn = None;
    ts.parser = MidiParser::new();
    ts.clock = MidiClockSync::new();
    ts.last_clock_pulse_at = None;
    ts.clock_bpm = 0.0;

    let Ok(input) = midir::MidiInput::new(CLIENT_NAME) else {
        eprintln!("[midi] failed to init MIDI input client while selecting '{name}'");
        return;
    };
    let Some(port) = find_port_by_name(&input, &name) else {
        eprintln!("[midi] input port '{name}' not found (unplugged?)");
        return;
    };

    let device_name = name.clone();
    let tx = midi_tx.clone();
    let connect_result = input.connect(
        &port,
        "opendrop-native-in",
        move |timestamp_us, data, _| {
            let _ = tx.send(RawMidi { device_name: device_name.clone(), timestamp_us, data: data.to_vec() });
        },
        (),
    );

    match connect_result {
        Ok(conn) => {
            ts.input_conn = Some((name.clone(), conn));
            ts.connected = true;
            if try_open_output(ts, &name) {
                ts.hotplug_epoch = ts.hotplug_epoch.wrapping_add(1);
            }
        }
        Err(e) => eprintln!("[midi] failed to connect to input port '{name}': {}", e.kind()),
    }
}

/// Sends raw on/off LED feedback for `id`'s mapped trigger, if: it's
/// mapped, the mapped device is the currently-selected input, a
/// name-matched output connection is open, and the trigger kind supports
/// LED feedback (not pitchbend). Any other case is a silent no-op:
/// mirrors every early `return` in `MidiEngine.sendFeedback`.
///
/// A `send()` failure (the expected outcome once the output device is
/// physically unplugged) drops `ts.output_conn` back to `None`: this is
/// what actually makes hotplug reconnection work: `check_hotplug` only
/// retries `try_open_output` while `ts.output_conn` is `None`, so a
/// connection that died silently (no error at unplug time, only at the
/// next failed `send`) would otherwise never be retried.
fn handle_push_led(ts: &mut ThreadState, id: CommandId, on: bool) {
    let Some(key) = ts.mapping.get(&id) else { return };
    let Some((input_name, _)) = &ts.input_conn else { return };
    if &key.device_id != input_name {
        return;
    }
    let Some(status) = led_status_byte(key.kind, key.channel) else { return };
    let Some((_, output)) = ts.output_conn.as_mut() else { return };
    let velocity = if on { 127 } else { 0 };
    if let Err(e) = output.send(&[status, key.number, velocity]) {
        eprintln!("[midi] LED feedback send failed: {e}");
        ts.output_conn = None; // let check_hotplug retry the reconnect
    }
}

/// Attempts to open the output port named `name`, storing the connection
/// in `ts.output_conn` on success. Used both by `SelectPort` (open the
/// matching output for a freshly-selected input) and `check_hotplug`
/// (reopen it once it reappears). Returns whether a connection was opened.
pub(super) fn try_open_output(ts: &mut ThreadState, name: &str) -> bool {
    let Ok(output) = midir::MidiOutput::new(CLIENT_NAME) else { return false };
    let Some(port) = find_port_by_name(&output, name) else { return false };
    match output.connect(&port, "opendrop-native-out") {
        Ok(conn) => {
            ts.output_conn = Some((name.to_string(), conn));
            true
        }
        Err(_) => false,
    }
}

pub(super) fn list_port_names<IO: midir::MidiIO>(io: &IO) -> Vec<String> {
    io.ports().iter().filter_map(|p| io.port_name(p).ok()).collect()
}

fn find_port_by_name<IO: midir::MidiIO>(io: &IO, name: &str) -> Option<IO::Port> {
    io.ports().into_iter().find(|p| io.port_name(p).map(|n| n == name).unwrap_or(false))
}

#[cfg(test)]
mod tests {
    use super::*;
    use opendrop_core::commands::CommandId;

    /// `handle_push_led` with nothing mapped must be a silent no-op:
    /// exercised without any real `midir` connection.
    #[test]
    fn push_led_on_unmapped_command_is_a_no_op() {
        let mut ts = ThreadState::new();
        handle_push_led(&mut ts, CommandId::Crossfader, true); // must not panic
    }

    /// Whole-branch review Finding M7: `Connect` only probes the backend
    /// and lists ports: it must never claim `connected`, regardless of
    /// whether a real MIDI backend happens to be available in this test
    /// environment. Only `SelectPort` actually opening a port may set it.
    #[test]
    fn connect_never_claims_a_port_is_open() {
        let mut ts = ThreadState::new();
        handle_connect(&mut ts);
        assert!(!ts.connected);
    }
}
