//! Input device selection: prefer the system output ("monitor") source,
//! fall back to the default input device.
//!
//! Override vs. the original plan: matching a device *name* containing
//! "monitor" (the PulseAudio convention) does not work on cpal 0.18's native
//! PipeWire backend: verified by reading the vendored source
//! (`cpal-0.18.2/src/host/pipewire/device.rs`). The name cpal exposes
//! (`Display`/`.to_string()`: `DeviceTrait` has no `.name()` method in cpal
//! 0.18) comes from PipeWire's `NODE_DESCRIPTION`, which is localized and
//! never contains "monitor". Nor does `default_output_device()` help: on
//! this backend it resolves to a distinct, output-only virtual alias
//! (`Device::output_default()`, `direction: Output`) that never appears in
//! `input_devices()`. What cpal *does* expose, always under this exact,
//! backend-defined (not PipeWire-derived, so locale-independent) name, is a
//! separate Duplex virtual alias for "the current default sink, capturable"
//!: `Device::sink_default()`, `description: "default_sink"`: which shows
//! up directly in `input_devices()`. That's the monitor source.
const DEFAULT_SINK_ALIAS: &str = "default_sink";

/// Picks the monitor source by its `list_input_devices` label, falling back
/// to the default input device.
///
/// Uses the fallible `description()` for the same reason
/// `list_input_devices` does (see its doc comment): `Display`/`.to_string()`
/// panics when `description()` fails, and a panic here kills the capture
/// thread outright, taking the Audio panel's device hot-swap down with it
/// for the rest of the session.
pub fn select_input_device(host: &cpal::Host) -> Option<cpal::Device> {
    use cpal::traits::{DeviceTrait, HostTrait};
    let monitor = host.input_devices().ok().and_then(|mut devices| {
        devices.find(|d| d.description().map(|desc| desc.name() == DEFAULT_SINK_ALIAS).unwrap_or(false))
    });
    if let Some(device) = monitor {
        return Some(device);
    }
    eprintln!("[audio] no \"{DEFAULT_SINK_ALIAS}\" (system-output monitor) input device: falling back to the default input device");
    host.default_input_device()
}

/// Labels of every available input device, for UI device pickers.
///
/// Uses the fallible `description()` (not a `.name()` method: `DeviceTrait`
/// has none in cpal 0.18, see module doc above: and deliberately not
/// `Display`/`.to_string()` either: `Display::fmt` on the pipewire backend
/// calls `description()` internally and turns its `Err` into `fmt::Error`,
/// which `ToString`'s blanket impl then turns into a panic via `.expect(...)`
///: reachable in practice, e.g. a device disconnected mid-enumeration). A
/// device whose `description()` fails is skipped, not treated as an error.
pub fn list_input_devices(host: &cpal::Host) -> Vec<String> {
    use cpal::traits::{DeviceTrait, HostTrait};
    host.input_devices()
        .map(|devices| devices.filter_map(|d| d.description().ok().map(|desc| desc.name().to_owned())).collect())
        .unwrap_or_default()
}

/// Selects an input device by its exact `list_input_devices` label, for
/// hot-swapping the capture device by name. A device whose `description()`
/// fails (disconnected) is treated as a non-match, never as an error.
pub fn select_input_device_by_name(host: &cpal::Host, name: &str) -> Option<cpal::Device> {
    use cpal::traits::{DeviceTrait, HostTrait};
    host.input_devices().ok()?.find(|d| d.description().map(|desc| desc.name() == name).unwrap_or(false))
}
