mod analysis;
mod capture;
pub mod device;

pub use analysis::AnalyserConfig;

/// Latest captured PCM chunk (interleaved stereo, always even length) + the
/// latest low-frequency energy computed by that same FFT block (AC-5: one
/// FFT feeds both). `energy_byte` isn't consumed yet in Phase 3 (wiring to
/// `core::beat_detector` is Phase 4+) but must be published now anyway (AC-7).
pub struct AudioSnapshot {
    pub pcm: Vec<f32>,
    pub energy_byte: f64,
}

/// Silent (zero-filled) stereo buffer until a real block has been captured,
/// or forever if capture fails at startup: never a crash, never a fallback
/// to `synth_audio_chunk` (AC-4).
const SILENT_PLACEHOLDER_FRAMES: usize = 480;
fn silent_snapshot() -> AudioSnapshot {
    AudioSnapshot { pcm: vec![0.0; SILENT_PLACEHOLDER_FRAMES * 2], energy_byte: 0.0 }
}

pub struct AudioHandle {
    snapshot: std::sync::Arc<arc_swap::ArcSwap<AudioSnapshot>>,
    device_tx: std::sync::mpsc::Sender<String>,
}

impl AudioHandle {
    /// Never blocks: an atomic load of the current Arc (AC-7).
    pub fn latest(&self) -> std::sync::Arc<AudioSnapshot> {
        self.snapshot.load_full()
    }

    /// Requests the capture thread to reopen its stream on the named input
    /// device. Never blocks and never fails on the caller's side: a closed
    /// receiver (capture thread gone) is silently ignored.
    pub fn set_device(&self, name: String) {
        let _ = self.device_tx.send(name);
    }
}

/// Spawns the dedicated capture thread and returns immediately. Infallible
/// by construction: any failure (no device, open refused) is logged once
/// internally and leaves `AudioHandle::latest()` returning silence
/// indefinitely: never a panic, never a `Result` for the caller to handle
/// (AC-4).
pub fn spawn_capture() -> AudioHandle {
    let snapshot = std::sync::Arc::new(arc_swap::ArcSwap::from_pointee(silent_snapshot()));
    let (device_tx, device_rx) = std::sync::mpsc::channel();
    capture::spawn(snapshot.clone(), device_rx);
    AudioHandle { snapshot, device_tx }
}

/// Lists the labels of every available input device, for UI device pickers.
pub fn list_input_devices() -> Vec<String> {
    device::list_input_devices(&cpal::default_host())
}
