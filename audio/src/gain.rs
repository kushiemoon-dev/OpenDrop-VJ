//! Input gain stage: a manual gain in dB plus an optional automatic gain
//! control (AGC) that levels a quiet or loud source toward a fixed peak.
//! Applied to every captured block before the FFT and before the PCM is
//! handed to the decks, so one setting drives both the energy used for beat
//! detection and the audio the visualizers react to.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

pub const GAIN_DB_RANGE: (f32, f32) = (-12.0, 24.0);

/// Peak level the AGC steers toward, and the limits of what it may do.
const AGC_TARGET_PEAK: f32 = 0.5;
const AGC_MIN_GAIN: f32 = 0.25;
const AGC_MAX_GAIN: f32 = 8.0;
/// Below this envelope the signal is treated as silence and is not boosted,
/// so room noise between tracks does not get amplified.
const AGC_SILENCE_FLOOR: f32 = 0.005;
/// Time constants, in seconds: fast to react to a louder passage, slow to
/// let go, so the gain does not pump on every beat.
const AGC_ATTACK_SECONDS: f32 = 0.05;
const AGC_RELEASE_SECONDS: f32 = 3.0;

/// Settings shared between the UI (writer) and the capture callback (reader).
pub struct GainSettings {
    gain_db_bits: AtomicU32,
    agc: AtomicBool,
}

impl Default for GainSettings {
    fn default() -> Self {
        Self { gain_db_bits: AtomicU32::new(0.0_f32.to_bits()), agc: AtomicBool::new(false) }
    }
}

impl GainSettings {
    pub fn set_gain_db(&self, db: f32) {
        let db = if db.is_finite() { db.clamp(GAIN_DB_RANGE.0, GAIN_DB_RANGE.1) } else { 0.0 };
        self.gain_db_bits.store(db.to_bits(), Ordering::Relaxed);
    }

    pub fn gain_db(&self) -> f32 {
        f32::from_bits(self.gain_db_bits.load(Ordering::Relaxed))
    }

    pub fn set_agc(&self, enabled: bool) {
        self.agc.store(enabled, Ordering::Relaxed);
    }

    pub fn agc(&self) -> bool {
        self.agc.load(Ordering::Relaxed)
    }
}

/// Tracks the signal envelope for the AGC. One per capture stream.
#[derive(Default)]
pub struct GainStage {
    envelope: f32,
}

impl GainStage {
    /// Scales `block` (interleaved samples) in place. `sample_rate` and
    /// `channels` give the block's duration so the AGC time constants hold
    /// whatever the driver's buffer size is.
    pub fn process(&mut self, settings: &GainSettings, block: &mut [f32], sample_rate: u32, channels: usize) {
        let manual = 10.0_f32.powf(settings.gain_db() / 20.0);
        let agc = if settings.agc() { self.agc_gain(block, sample_rate, channels) } else { self.reset_envelope() };
        let gain = manual * agc;
        if (gain - 1.0).abs() < f32::EPSILON {
            return;
        }
        for sample in block.iter_mut() {
            *sample = (*sample * gain).clamp(-1.0, 1.0);
        }
    }

    fn reset_envelope(&mut self) -> f32 {
        self.envelope = 0.0;
        1.0
    }

    fn agc_gain(&mut self, block: &[f32], sample_rate: u32, channels: usize) -> f32 {
        let peak = block.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        let seconds = block.len() as f32 / (sample_rate.max(1) as f32 * channels.max(1) as f32);
        let tau = if peak > self.envelope { AGC_ATTACK_SECONDS } else { AGC_RELEASE_SECONDS };
        self.envelope += (peak - self.envelope) * (1.0 - (-seconds / tau).exp());
        if self.envelope < AGC_SILENCE_FLOOR {
            return 1.0;
        }
        (AGC_TARGET_PEAK / self.envelope).clamp(AGC_MIN_GAIN, AGC_MAX_GAIN)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn tone(amplitude: f32, frames: usize) -> Vec<f32> {
        (0..frames * 2).map(|i| amplitude * ((i / 2) as f32 * 0.05).sin()).collect()
    }

    fn peak(block: &[f32]) -> f32 {
        block.iter().fold(0.0, |m, s| m.max(s.abs()))
    }

    #[test]
    fn manual_gain_of_six_db_roughly_doubles_the_level() {
        let settings = GainSettings::default();
        settings.set_gain_db(6.0);
        let mut block = tone(0.2, 480);
        let before = peak(&block);

        GainStage::default().process(&settings, &mut block, RATE, 2);

        assert!((peak(&block) / before - 1.995).abs() < 0.01);
    }

    #[test]
    fn output_never_exceeds_full_scale() {
        let settings = GainSettings::default();
        settings.set_gain_db(24.0);
        let mut block = tone(0.9, 480);

        GainStage::default().process(&settings, &mut block, RATE, 2);

        assert!(peak(&block) <= 1.0);
    }

    #[test]
    fn gain_is_clamped_and_non_finite_input_resets() {
        let settings = GainSettings::default();
        settings.set_gain_db(500.0);
        assert_eq!(settings.gain_db(), GAIN_DB_RANGE.1);
        settings.set_gain_db(f32::NAN);
        assert_eq!(settings.gain_db(), 0.0);
    }

    #[test]
    fn agc_brings_a_quiet_source_up_toward_the_target() {
        let settings = GainSettings::default();
        settings.set_agc(true);
        let mut stage = GainStage::default();
        let mut last = tone(0.05, 480);
        for _ in 0..200 {
            last = tone(0.05, 480);
            stage.process(&settings, &mut last, RATE, 2);
        }
        assert!(peak(&last) > 0.2, "peak was {}", peak(&last));
        assert!(peak(&last) <= 0.5 + 0.05);
    }

    #[test]
    fn agc_leaves_silence_alone() {
        let settings = GainSettings::default();
        settings.set_agc(true);
        let mut stage = GainStage::default();
        for _ in 0..50 {
            let mut block = tone(0.001, 480);
            stage.process(&settings, &mut block, RATE, 2);
            assert!(peak(&block) <= 0.0011);
        }
    }

    #[test]
    fn agc_pulls_a_loud_source_down() {
        let settings = GainSettings::default();
        settings.set_agc(true);
        let mut stage = GainStage::default();
        let mut last = tone(0.95, 480);
        for _ in 0..50 {
            last = tone(0.95, 480);
            stage.process(&settings, &mut last, RATE, 2);
        }
        assert!(peak(&last) < 0.7, "peak was {}", peak(&last));
    }
}
