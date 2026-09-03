//! Port of OpenDrop-VJ `src/lib/engine/bpm.ts`: pure, zero-I/O beat
//! detection from a stream of low-frequency energy samples. Same decoupling
//! as `core::clock`'s `step`/`pulse`: no callback, returns the result to the
//! caller instead of invoking an `onBeat`.

pub struct BeatDetector {
    energy_history: [f64; 43],
    history_write_idx: usize,
    beat_intervals: std::collections::VecDeque<f64>, // cap 8
    last_beat_time_ms: f64, // 0.0 initial, like bpm.ts's lastBeatTime
    bpm: f64,               // 0.0 until the first valid commit, never reset after
}

pub struct BeatDetectionResult {
    pub beat_triggered: bool,
    pub bpm: f64, // BeatDetector::bpm() as of this call, committed or not
}

impl BeatDetector {
    pub fn new() -> Self {
        Self {
            energy_history: [0.0; 43],
            history_write_idx: 0,
            beat_intervals: std::collections::VecDeque::new(),
            last_beat_time_ms: 0.0,
            bpm: 0.0,
        }
    }

    pub fn bpm(&self) -> f64 {
        self.bpm
    }

    fn avg(&self) -> f64 {
        self.energy_history.iter().sum::<f64>() / 43.0
    }

    /// One low-frequency energy sample + a monotonic timestamp (ms, caller's
    /// choice of origin; `core` owns no timer). Direct port of
    /// bpm.ts:41-78 (`_tick`).
    pub fn process_sample(&mut self, energy: f64, now_ms: f64) -> BeatDetectionResult {
        self.energy_history[self.history_write_idx] = energy;
        self.history_write_idx = (self.history_write_idx + 1) % 43;
        let avg = self.avg();

        let mut beat_triggered = false;
        if energy > avg * 1.35 && avg > 8.0 && now_ms - self.last_beat_time_ms > 300.0 {
            let interval = now_ms - self.last_beat_time_ms;
            self.last_beat_time_ms = now_ms;
            beat_triggered = true;
            if interval > 270.0 && interval < 1000.0 {
                self.beat_intervals.push_back(interval);
                if self.beat_intervals.len() > 8 {
                    self.beat_intervals.pop_front();
                }
                if self.beat_intervals.len() >= 2 {
                    let avg_interval: f64 =
                        self.beat_intervals.iter().sum::<f64>() / self.beat_intervals.len() as f64;
                    let bpm = (60_000.0 / avg_interval).round();
                    if (60.0..=220.0).contains(&bpm) {
                        self.bpm = bpm;
                    }
                }
            }
        }
        BeatDetectionResult { beat_triggered, bpm: self.bpm }
    }
}

impl Default for BeatDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod rolling_average_convergence {
        use super::*;

        #[test]
        fn converges_to_the_sample_value_only_after_43_identical_samples() {
            let mut d = BeatDetector::new();
            for i in 0..42 {
                d.process_sample(10.0, i as f64 * 10.0);
            }
            // One zeroed slot still drags the average below the sample value.
            assert!((d.avg() - 10.0).abs() > 0.1);
            d.process_sample(10.0, 420.0);
            // Every slot now holds 10.0: fully converged.
            assert!((d.avg() - 10.0).abs() < 1e-9);
        }
    }

    mod threshold_and_floor_gating {
        use super::*;

        #[test]
        fn high_relative_energy_does_not_trigger_while_avg_is_still_below_the_silence_floor() {
            let mut d = BeatDetector::new();
            // avg = 300.0/43 ≈ 6.98, below the 8.0 floor, even though
            // 300.0 > avg*1.35 easily holds.
            let r = d.process_sample(300.0, 1000.0);
            assert!(!r.beat_triggered);
        }

        #[test]
        fn energy_at_or_below_avg_times_1_35_does_not_trigger() {
            let mut d = BeatDetector::new();
            for i in 0..43 {
                d.process_sample(10.0, i as f64 * 10.0);
            }
            // avg == 10.0 here; a repeat of the same value never exceeds avg*1.35.
            let r = d.process_sample(10.0, 10_000.0);
            assert!(!r.beat_triggered);
        }
    }

    mod cooldown {
        use super::*;

        #[test]
        fn a_second_trigger_within_300ms_does_not_retrigger() {
            let mut d = BeatDetector::new();
            for i in 0..43 {
                d.process_sample(10.0, i as f64 * 10.0);
            }
            let first = d.process_sample(50.0, 10_000.0);
            assert!(first.beat_triggered);
            let second = d.process_sample(50.0, 10_100.0);
            assert!(!second.beat_triggered);
        }
    }

    mod implausible_interval_rejection {
        use super::*;

        #[test]
        fn a_trigger_at_an_implausible_interval_still_reports_triggered_but_never_commits_bpm() {
            let mut d = BeatDetector::new();
            for i in 0..43 {
                d.process_sample(10.0, i as f64 * 10.0);
            }
            let first = d.process_sample(50.0, 10_000.0);
            assert!(first.beat_triggered);
            // 1001ms >= 1000ms: outside the (270, 1000) plausibility window.
            let second = d.process_sample(50.0, 11_001.0);
            assert!(second.beat_triggered);
            assert_eq!(second.bpm, 0.0);
        }
    }

    mod bpm_clamping {
        use super::*;

        #[test]
        fn two_plausible_intervals_commit_a_bpm_inside_60_to_220() {
            let mut d = BeatDetector::new();
            for i in 0..43 {
                d.process_sample(10.0, i as f64 * 10.0);
            }
            let mut now = 10_000.0;
            let beat1 = d.process_sample(50.0, now); // interval from t=0: implausible, no commit
            assert!(beat1.beat_triggered);
            assert_eq!(beat1.bpm, 0.0);

            now += 310.0; // plausible interval, but only 1 in the deque: no commit yet
            let beat2 = d.process_sample(50.0, now);
            assert!(beat2.beat_triggered);
            assert_eq!(beat2.bpm, 0.0);

            now += 310.0; // 2nd plausible interval: avg_interval = 310 -> bpm = round(60000/310) = 194
            let beat3 = d.process_sample(50.0, now);
            assert!(beat3.beat_triggered);
            assert_eq!(beat3.bpm, 194.0);
        }

        #[test]
        fn a_computed_bpm_outside_60_to_220_is_not_committed() {
            let mut d = BeatDetector::new();
            for i in 0..43 {
                d.process_sample(10.0, i as f64 * 10.0);
            }
            let mut now = 10_000.0;
            d.process_sample(50.0, now); // implausible first interval, no commit

            now += 271.0; // plausible (>270), 1 in the deque: no commit yet
            let beat2 = d.process_sample(50.0, now);
            assert_eq!(beat2.bpm, 0.0);

            now += 271.0; // avg_interval = 271 -> round(60000/271) = 221, outside 60..=220
            let beat3 = d.process_sample(50.0, now);
            assert!(beat3.beat_triggered);
            assert_eq!(beat3.bpm, 0.0); // rejected: stays at its previous value
        }
    }
}
