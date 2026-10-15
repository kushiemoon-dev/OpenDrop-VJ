//! Input device selection: prefer the system output ("monitor") source,
//! fall back to the default input device.
//!
//! Override vs. the original plan: matching a device *name* containing
//! "monitor" (the PulseAudio convention) does not work on cpal 0.18's native
//! PipeWire backend, verified by reading the vendored source
//! (`cpal-0.18.2/src/host/pipewire/device.rs`). The name cpal exposes
//! (`Display`/`.to_string()`, since `DeviceTrait` has no `.name()` method in cpal
//! 0.18) comes from PipeWire's `NODE_DESCRIPTION`, which is localized and
//! never contains "monitor". Nor does `default_output_device()` help: on
//! this backend it resolves to a distinct, output-only virtual alias
//! (`Device::output_default()`, `direction: Output`) that never appears in
//! `input_devices()`. What cpal *does* expose, always under this exact,
//! backend-defined (not PipeWire-derived, so locale-independent) name, is a
//! separate Duplex virtual alias for "the current default sink, capturable":
//! `Device::sink_default()`, `description: "default_sink"`, which shows
//! up directly in `input_devices()`. That's the monitor source.
//!
//! None of this applies on Windows: WASAPI has no such alias, but opening a
//! WASAPI *output* device as an input transparently enables loopback mode
//! (confirmed in the vendored comment in
//! `cpal-0.18.2/src/host/wasapi/mod.rs`), so the output device itself is the
//! capture source there, picked straight from `output_devices()`.
//! Microphones are listed too, from `input_devices()`, marked with a suffix.
#[cfg(not(target_os = "windows"))]
const DEFAULT_SINK_ALIAS: &str = "default_sink";

/// Picks the monitor source by its `list_input_devices` label, falling back
/// to the default input device.
///
/// Uses the fallible `description()` for the same reason
/// `list_input_devices` does (see its doc comment): `Display`/`.to_string()`
/// panics when `description()` fails, and a panic here kills the capture
/// thread outright, taking the Audio panel's device hot-swap down with it
/// for the rest of the session.
#[cfg(not(target_os = "windows"))]
pub fn select_input_device(host: &cpal::Host) -> Option<cpal::Device> {
    use cpal::traits::{DeviceTrait, HostTrait};
    let monitor = host.input_devices().ok().and_then(|mut devices| {
        devices.find(|d| d.description().map(|desc| desc.name() == DEFAULT_SINK_ALIAS).unwrap_or(false))
    });
    if let Some(device) = monitor {
        return Some(device);
    }
    opendrop_core::log_error!("[audio] no \"{DEFAULT_SINK_ALIAS}\" (system-output monitor) input device, falling back to the default input device");
    host.default_input_device()
}

/// Windows: no alias needed, the default output device loopback-captures
/// directly (see module doc).
#[cfg(target_os = "windows")]
pub fn select_input_device(host: &cpal::Host) -> Option<cpal::Device> {
    use cpal::traits::HostTrait;
    host.default_output_device()
}

/// Labels of every available input device, for UI device pickers.
///
/// Uses the fallible `description()` (not a `.name()` method, since `DeviceTrait`
/// has none in cpal 0.18, see module doc above) and deliberately not
/// `Display`/`.to_string()` either: `Display::fmt` on the pipewire backend
/// calls `description()` internally and turns its `Err` into `fmt::Error`,
/// which `ToString`'s blanket impl then turns into a panic via `.expect(...)`,
/// reachable in practice, e.g. a device disconnected mid-enumeration. A
/// device whose `description()` fails is skipped, not treated as an error.
#[cfg(not(target_os = "windows"))]
pub fn list_input_devices(host: &cpal::Host) -> Vec<String> {
    use cpal::traits::HostTrait;
    host.input_devices().map(|devices| devices.filter_map(|d| device_label(&d)).collect()).unwrap_or_default()
}

/// Joins a device's human description with its unique node id, because a
/// microphone and the monitor of the same card share one description and
/// would otherwise be indistinguishable (and always resolve to the first).
/// Only the capture path outside Windows uses it, plus the tests.
#[cfg(any(not(target_os = "windows"), test))]
fn format_label(description: &str, id: &str) -> String {
    format!("{description} [{id}]")
}

#[cfg(not(target_os = "windows"))]
fn device_label(d: &cpal::Device) -> Option<String> {
    use cpal::traits::DeviceTrait;
    let desc = d.description().ok()?;
    let id = d.id().ok()?;
    Some(format_label(desc.name(), id.id()))
}

/// Windows marks a microphone with this suffix. Output devices (captured as
/// loopback) keep their bare name, so a choice saved by an earlier version
/// still resolves.
#[cfg(any(target_os = "windows", test))]
const MIC_SUFFIX: &str = " (microphone)";

#[cfg(any(target_os = "windows", test))]
fn microphone_label(name: &str) -> String {
    format!("{name}{MIC_SUFFIX}")
}

/// `(device name, is a microphone)` for a label from `list_input_devices`.
#[cfg(any(target_os = "windows", test))]
fn parse_label(label: &str) -> (&str, bool) {
    match label.strip_suffix(MIC_SUFFIX) {
        Some(name) => (name, true),
        None => (label, false),
    }
}

/// Windows: the output devices (captured as loopback, what is playing) and
/// the microphones, which are marked with a suffix (see module doc).
#[cfg(target_os = "windows")]
pub fn list_input_devices(host: &cpal::Host) -> Vec<String> {
    use cpal::traits::{DeviceTrait, HostTrait};
    let name_of = |d: cpal::Device| d.description().ok().map(|desc| desc.name().to_owned());
    let mut labels: Vec<String> = host.output_devices().map(|devices| devices.filter_map(name_of).collect()).unwrap_or_default();
    labels.extend(
        host.input_devices()
            .map(|devices| devices.filter_map(name_of).map(|name| microphone_label(&name)).collect::<Vec<_>>())
            .unwrap_or_default(),
    );
    labels
}

/// Selects an input device by its exact `list_input_devices` label, for
/// hot-swapping the capture device by name. A device whose `description()`
/// fails (disconnected) is treated as a non-match, never as an error.
#[cfg(not(target_os = "windows"))]
pub fn select_input_device_by_name(host: &cpal::Host, name: &str) -> Option<cpal::Device> {
    use cpal::traits::HostTrait;
    host.input_devices().ok()?.find(|d| device_label(d).as_deref() == Some(name))
}

/// Windows: matches against the same two sets `list_input_devices` lists
/// (see module doc).
#[cfg(target_os = "windows")]
pub fn select_input_device_by_name(host: &cpal::Host, name: &str) -> Option<cpal::Device> {
    use cpal::traits::{DeviceTrait, HostTrait};
    let (device_name, microphone) = parse_label(name);
    let matches = |d: &cpal::Device| d.description().map(|desc| desc.name() == device_name).unwrap_or(false);
    if microphone {
        host.input_devices().ok()?.find(matches)
    } else {
        host.output_devices().ok()?.find(matches)
    }
}

#[cfg(test)]
mod tests {
    use super::{format_label, microphone_label, parse_label};

    #[test]
    fn a_microphone_label_round_trips_and_an_output_label_stays_bare() {
        assert_eq!(parse_label(&microphone_label("Microphone (Realtek)")), ("Microphone (Realtek)", true));
        assert_eq!(parse_label("Speakers (Realtek)"), ("Speakers (Realtek)", false));
    }

    #[test]
    fn a_mic_and_an_output_with_the_same_name_get_different_labels() {
        assert_ne!(microphone_label("USB Audio"), "USB Audio");
    }

    #[test]
    fn same_description_with_different_ids_gives_distinct_labels() {
        let mic = format_label("PCM2902 Audio Codec", "alsa_input.usb");
        let monitor = format_label("PCM2902 Audio Codec", "alsa_output.usb.monitor");
        assert_ne!(mic, monitor);
    }
}
