//! Port of OpenDrop-VJ `src/lib/engine/clock.ts`: the pure phase-math half only.
//!
//! `start()`/`stop()` (the `requestAnimationFrame` loop) are deliberately NOT
//! ported here: `core` has zero I/O, and driving a frame loop belongs to a
//! later `app` crate, which will call `step()` once per real frame instead.
//! The TS `onBeat`/`onTick` callback lists are likewise dropped in favor of
//! `step`/`pulse`/`sync_external` returning the beat count they fired,
//! callback-free and trivial to assert on in tests.

#[derive(Default)]
pub struct Clock {
    bpm: f64,
    phase01: f64,
    beat_count: u64,
}

impl Clock {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bpm(&self) -> f64 {
        self.bpm
    }

    pub fn phase01(&self) -> f64 {
        self.phase01
    }

    pub fn beat_count(&self) -> u64 {
        self.beat_count
    }

    pub fn set_bpm(&mut self, bpm: f64) {
        self.bpm = bpm.clamp(0.0, 300.0);
    }

    /// Sync phase to 0: called by external sources (audio detector, tap-tempo).
    /// In pulse-only mode (bpm == 0) also emits a beat immediately.
    /// Returns the number of beats emitted (0 or 1).
    pub fn pulse(&mut self, bpm: Option<f64>) -> u32 {
        if let Some(bpm) = bpm {
            self.set_bpm(bpm);
        }
        self.phase01 = 0.0;
        if self.bpm == 0.0 {
            self.emit_beat();
            1
        } else {
            0
        }
    }

    /// Force BPM and phase from an external source (e.g. Ableton Link).
    /// Emits a beat if phase wrapped backward since the last call.
    /// Returns the number of beats emitted (0 or 1).
    pub fn sync_external(&mut self, bpm: f64, phase01: f64) -> u32 {
        self.set_bpm(bpm);
        let prev = self.phase01;
        self.phase01 = phase01;
        if phase01 < prev - 0.5 {
            self.emit_beat();
            1
        } else {
            0
        }
    }

    /// Advance the clock by dt seconds. Returns the number of beats emitted.
    pub fn step(&mut self, dt_seconds: f64) -> u32 {
        let mut beats = 0;
        if self.bpm > 0.0 {
            self.phase01 += dt_seconds * self.bpm / 60.0;
            while self.phase01 >= 1.0 {
                self.phase01 -= 1.0;
                self.emit_beat();
                beats += 1;
            }
        }
        beats
    }

    fn emit_beat(&mut self) {
        self.beat_count += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod set_bpm_and_phase_advancement {
        use super::*;

        #[test]
        fn phase_advances_proportionally_to_bpm() {
            let mut clock = Clock::new();
            clock.set_bpm(120.0); // 2 beats/sec, phase += 2*dt
            clock.step(0.25); // dt=0.25s -> phase += 0.5
            assert!((clock.phase01() - 0.5).abs() < 1e-9);
        }

        #[test]
        fn phase_stays_at_0_if_bpm_is_0() {
            let mut clock = Clock::new();
            clock.step(1.0);
            assert_eq!(clock.phase01(), 0.0);
            assert_eq!(clock.beat_count(), 0);
        }

        #[test]
        fn emits_a_beat_when_phase_exceeds_1() {
            let mut clock = Clock::new();
            clock.set_bpm(120.0);
            let beats = clock.step(0.5); // phase -> 1.0 -> 0.0, beat emitted
            assert_eq!(beats, 1);
            assert_eq!(clock.beat_count(), 1);
            assert!(clock.phase01().abs() < 1e-9);
        }

        #[test]
        fn emits_multiple_beats_in_a_single_large_step() {
            let mut clock = Clock::new();
            clock.set_bpm(120.0); // 2 beats/sec
            let beats = clock.step(2.5); // 2.5s * 2bps = 5 beats, phase stays at 0
            assert_eq!(beats, 5);
            assert_eq!(clock.beat_count(), 5);
            assert!(clock.phase01().abs() < 1e-9);
        }

        #[test]
        fn set_bpm_clamps_0_to_300() {
            let mut clock = Clock::new();
            clock.set_bpm(-10.0);
            assert_eq!(clock.bpm(), 0.0);
            clock.set_bpm(500.0);
            assert_eq!(clock.bpm(), 300.0);
        }
    }

    mod pulse {
        use super::*;

        #[test]
        fn pulse_with_bpm_updates_the_bpm_and_resets_phase_to_0() {
            let mut clock = Clock::new();
            clock.set_bpm(120.0);
            clock.step(0.3);
            assert!(clock.phase01() > 0.0);
            let beats = clock.pulse(Some(140.0));
            assert_eq!(clock.bpm(), 140.0);
            assert_eq!(clock.phase01(), 0.0);
            assert_eq!(beats, 0);
        }

        #[test]
        fn pulse_without_bpm_in_bpm_0_mode_emits_an_immediate_beat() {
            let mut clock = Clock::new();
            let beats = clock.pulse(None); // bpm=0 -> emits immediately
            assert_eq!(beats, 1);
            assert_eq!(clock.beat_count(), 1);
        }

        #[test]
        fn pulse_with_bpm_greater_than_0_does_not_double_emit() {
            let mut clock = Clock::new();
            clock.set_bpm(120.0);
            let beats = clock.pulse(Some(120.0)); // phase resync only, no immediate emission
            assert_eq!(beats, 0);
        }
    }

    mod sync_external {
        use super::*;

        #[test]
        fn emits_a_beat_when_phase_wraps_backward() {
            let mut clock = Clock::new();
            clock.sync_external(120.0, 0.9);
            let beats = clock.sync_external(120.0, 0.1); // wrapped backward past the 0.5 threshold
            assert_eq!(beats, 1);
            assert_eq!(clock.beat_count(), 1);
            assert_eq!(clock.phase01(), 0.1);
        }

        #[test]
        fn does_not_emit_when_phase_advances_forward() {
            let mut clock = Clock::new();
            clock.sync_external(120.0, 0.1);
            let beats = clock.sync_external(120.0, 0.4);
            assert_eq!(beats, 0);
            assert_eq!(clock.beat_count(), 0);
        }
    }

    mod tick {
        use super::*;

        #[test]
        fn step_reports_phase_and_beat_count_on_every_call() {
            let mut clock = Clock::new();
            clock.set_bpm(60.0); // 1 beat/sec
            let beats1 = clock.step(0.5); // phase = 0.5, 0 beats
            assert!((clock.phase01() - 0.5).abs() < 1e-9);
            assert_eq!(clock.beat_count(), 0);
            assert_eq!(beats1, 0);
            let beats2 = clock.step(0.5); // phase = 0.0, 1 beat
            assert!(clock.phase01().abs() < 1e-9);
            assert_eq!(clock.beat_count(), 1);
            assert_eq!(beats2, 1);
        }
    }
}
