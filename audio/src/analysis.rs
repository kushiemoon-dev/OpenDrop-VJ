//! Port of OpenDrop-VJ `audio.ts` (AnalyserNode config + magnitude→byte
//! mapping) and `bpm.ts:47-51` (bass-band energy): FFT-backed analysis
//! decoupled from cpal capture, so it stays testable without real hardware.

pub const FFT_SIZE: usize = 2048;
pub const BIN_COUNT: usize = FFT_SIZE / 2; // 1024 == AnalyserNode.frequencyBinCount for fftSize=2048

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnalyserConfig {
    pub min_decibels: f64,
    pub max_decibels: f64,
    pub smoothing_time_constant: f64,
}

impl Default for AnalyserConfig {
    fn default() -> Self {
        Self { min_decibels: -100.0, max_decibels: -30.0, smoothing_time_constant: 0.8 }
    }
}

/// Port of the magnitude→byte mapping of `getByteFrequencyData` (Web Audio
/// spec): dB = 20*log10(magnitude), clamped to [min_decibels, max_decibels],
/// scaled to 0-255. `log10(0.0) == -inf` in Rust (no panic, no NaN): the
/// clamp right after absorbs it, no separate epsilon guard needed.
pub fn magnitude_to_byte(magnitude: f64, cfg: &AnalyserConfig) -> u8 {
    let db = (20.0 * magnitude.log10()).clamp(cfg.min_decibels, cfg.max_decibels);
    (255.0 / (cfg.max_decibels - cfg.min_decibels) * (db - cfg.min_decibels)) as u8
}

/// sqrt(mean(byte^2)) over the bins given: the caller already passes the
/// bass_end-sized sub-slice (bpm.ts:48-51).
pub fn bass_energy(bass_bins: &[u8]) -> f64 {
    let sum_sq: f64 = bass_bins.iter().map(|&b| (b as f64).powi(2)).sum();
    (sum_sq / bass_bins.len() as f64).sqrt()
}

pub struct Analyser {
    fft: std::sync::Arc<dyn rustfft::Fft<f32>>,
    hann_window: Vec<f32>,       // precomputed once, length FFT_SIZE
    rolling: Vec<f32>,           // length FFT_SIZE, latest captured mono samples
    smoothed_magnitude: Vec<f64>, // length BIN_COUNT, persists across calls (smoothing)
    config: AnalyserConfig,
}

impl Analyser {
    pub fn new(config: AnalyserConfig) -> Self {
        let fft = rustfft::FftPlanner::<f32>::new().plan_fft_forward(FFT_SIZE);
        let hann_window: Vec<f32> = (0..FFT_SIZE)
            .map(|n| 0.5 - 0.5 * ((std::f32::consts::TAU * n as f32) / (FFT_SIZE as f32 - 1.0)).cos())
            .collect();
        Self {
            fft,
            hann_window,
            rolling: vec![0.0; FFT_SIZE],
            smoothed_magnitude: vec![0.0; BIN_COUNT],
            config,
        }
    }

    /// Advances the rolling window with freshly captured `mono_samples` (one
    /// call per captured cpal block: see AC-5), runs one FFT + one
    /// smoothing pass, and returns this block's low-frequency energy.
    /// `mono_samples.len()` can be anything (the negotiated cpal block size,
    /// independent of FFT_SIZE).
    pub fn process(&mut self, mono_samples: &[f32]) -> f64 {
        let n = mono_samples.len().min(FFT_SIZE);
        let tail = &mono_samples[mono_samples.len() - n..];
        self.rolling.copy_within(n.., 0);
        self.rolling[FFT_SIZE - n..].copy_from_slice(tail);

        let mut buffer: Vec<rustfft::num_complex::Complex32> = self
            .rolling
            .iter()
            .zip(&self.hann_window)
            .map(|(&s, &w)| rustfft::num_complex::Complex32::new(s * w, 0.0))
            .collect();
        self.fft.process(&mut buffer);

        let smoothing = self.config.smoothing_time_constant;
        let mut byte = vec![0u8; BIN_COUNT];
        for k in 0..BIN_COUNT {
            let magnitude = buffer[k].norm() as f64;
            self.smoothed_magnitude[k] = smoothing * self.smoothed_magnitude[k] + (1.0 - smoothing) * magnitude;
            byte[k] = magnitude_to_byte(self.smoothed_magnitude[k], &self.config);
        }

        let bass_end = ((BIN_COUNT as f64) * 0.05).floor().max(1.0) as usize;
        bass_energy(&byte[..bass_end])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod magnitude_to_byte_tests {
        use super::*;

        #[test]
        fn saturates_to_0_at_or_below_min_decibels() {
            let cfg = AnalyserConfig::default();
            // magnitude so small its dB is far below min_decibels.
            assert_eq!(magnitude_to_byte(0.0, &cfg), 0);
            assert_eq!(magnitude_to_byte(1e-12, &cfg), 0);
        }

        #[test]
        fn saturates_to_255_at_or_above_max_decibels() {
            let cfg = AnalyserConfig::default();
            // magnitude=1.0 -> dB=0, above max_decibels=-30.0.
            assert_eq!(magnitude_to_byte(1.0, &cfg), 255);
        }

        #[test]
        fn maps_a_midpoint_decibel_value_to_a_plausible_middle_byte() {
            let cfg = AnalyserConfig::default();
            // dB = -65.0 is the midpoint of [-100, -30] -> byte 127 or 128.
            let magnitude = 10f64.powf(-65.0 / 20.0);
            let byte = magnitude_to_byte(magnitude, &cfg);
            assert!((120..=135).contains(&byte));
        }
    }

    mod bass_energy_tests {
        use super::*;

        #[test]
        fn returns_the_constant_value_when_every_bin_is_equal() {
            let bins = [40u8; 51];
            assert_eq!(bass_energy(&bins), 40.0);
        }

        #[test]
        fn returns_0_for_silence() {
            let bins = [0u8; 51];
            assert_eq!(bass_energy(&bins), 0.0);
        }
    }
}
