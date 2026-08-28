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

pub fn select_input_device(host: &cpal::Host) -> Option<cpal::Device> {
    use cpal::traits::HostTrait;
    let monitor =
        host.input_devices().ok().and_then(|mut devices| devices.find(|d| d.to_string() == DEFAULT_SINK_ALIAS));
    if let Some(device) = monitor {
        return Some(device);
    }
    eprintln!("[audio] no \"{DEFAULT_SINK_ALIAS}\" (system-output monitor) input device: falling back to the default input device");
    host.default_input_device()
}

/// Labels of every available input device, for UI device pickers.
///
/// Uses `Display`/`.to_string()`, not a `.name()` method: `DeviceTrait` has
/// none in cpal 0.18 (see module doc above). Same label `select_input_device`
/// matches `DEFAULT_SINK_ALIAS` against.
pub fn list_input_devices(host: &cpal::Host) -> Vec<String> {
    use cpal::traits::HostTrait;
    host.input_devices().map(|devices| devices.map(|d| d.to_string()).collect()).unwrap_or_default()
}

/// Selects an input device by its exact `list_input_devices` label, for
/// hot-swapping the capture device by name.
pub fn select_input_device_by_name(host: &cpal::Host, name: &str) -> Option<cpal::Device> {
    use cpal::traits::HostTrait;
    host.input_devices().ok()?.find(|d| d.to_string() == name)
}
