//! MIDI clock pulse -> BPM computation, mirroring
//! `midi-connection-actions.ts:104-136` exactly. Pure logic: no timers, no
//! I/O: the caller supplies pulse timestamps (ms) and owns the 2000ms
//! inactivity timeout that drives [`MidiClockSync::on_timeout`].

/// Ring buffer capacity for clock pulse timestamps (mirrors the JS
/// `clockTsRing` cap of 49).
const RING_CAPACITY: usize = 49;
/// MIDI clock pulses per quarter note.
const PULSES_PER_QUARTER: u32 = 24;
/// BPM is recomputed every `BPM_UPDATE_INTERVAL` pulses.
const BPM_UPDATE_INTERVAL: u32 = 6;
/// Timestamps needed to compute BPM (6 intervals from the last 7 samples).
const BPM_SAMPLE_COUNT: usize = 7;

pub struct MidiClockSync {
    pulses: u32,
    ts_ring: Vec<f64>,
}

impl MidiClockSync {
    pub fn new() -> Self {
        Self {
            pulses: 0,
            ts_ring: Vec::with_capacity(RING_CAPACITY),
        }
    }

    /// Called on every `0xF8` clock byte. Returns `(bpm, beat_fired)`:
    ///
    /// - `bpm` is `Some(value)` every 6th pulse, once at least 7 timestamps
    ///   are available, computed from the average of the last 6 intervals
    ///   and clamped to the plausible range 40.0..=300.0 by omission: an
    ///   out-of-range result yields `None` for that call (the previous BPM
    ///   value stands; this method does not track or return it).
    /// - `beat_fired` is `true` every 24th pulse (one quarter note),
    ///   independent of the BPM update cadence above.
    pub fn on_pulse(&mut self, now_ms: f64) -> (Option<f64>, bool) {
        self.ts_ring.push(now_ms);
        if self.ts_ring.len() > RING_CAPACITY {
            self.ts_ring.remove(0);
        }
        self.pulses += 1;

        let bpm = if self.pulses.is_multiple_of(BPM_UPDATE_INTERVAL) && self.ts_ring.len() >= BPM_SAMPLE_COUNT
        {
            let recent = &self.ts_ring[self.ts_ring.len() - BPM_SAMPLE_COUNT..];
            let interval_sum: f64 = recent.windows(2).map(|w| w[1] - w[0]).sum();
            let avg_interval_ms = interval_sum / (BPM_SAMPLE_COUNT - 1) as f64;
            let bpm = (60000.0 / (avg_interval_ms * 24.0) * 10.0).round() / 10.0;
            (40.0..=300.0).contains(&bpm).then_some(bpm)
        } else {
            None
        };

        let beat_fired = self.pulses.is_multiple_of(PULSES_PER_QUARTER);

        (bpm, beat_fired)
    }

    /// Called by the caller's own 2000ms-no-pulse inactivity timer (this
    /// type owns no timer/clock of its own). Resets internal state so the
    /// next `on_pulse` starts a fresh ring. Reporting `bpm = 0.0` to the UI
    /// is the caller's responsibility, not this method's.
    pub fn on_timeout(&mut self) {
        self.pulses = 0;
        self.ts_ring.clear();
    }
}

impl Default for MidiClockSync {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feeds `count` synthetic clock pulses at exactly `interval_ms` apart,
    /// starting at `t0`, returning the `(bpm, beat_fired)` result of every
    /// call in order.
    fn feed_pulses(
        sync: &mut MidiClockSync,
        t0: f64,
        interval_ms: f64,
        count: u32,
    ) -> Vec<(Option<f64>, bool)> {
        (0..count)
            .map(|i| sync.on_pulse(t0 + interval_ms * i as f64))
            .collect()
    }

    #[test]
    fn converges_to_120_bpm_from_synthetic_timestamps() {
        let interval_ms = 60_000.0 / (120.0 * 24.0); // ~20.8333ms
        let mut sync = MidiClockSync::new();
        let results = feed_pulses(&mut sync, 0.0, interval_ms, 48);

        // Every pulse that is a multiple of 6 and has >=7 samples available
        // should report a bpm converged on 120.0 within +/- 0.1.
        for (i, (bpm, _)) in results.iter().enumerate() {
            let pulse = i as u32 + 1;
            if pulse.is_multiple_of(6) && pulse >= 7 {
                let bpm = bpm.expect("expected a bpm value at this pulse");
                assert!(
                    (bpm - 120.0).abs() <= 0.1,
                    "pulse {pulse}: bpm {bpm} not within 0.1 of 120.0"
                );
            }
        }
    }

    #[test]
    fn beat_fired_exactly_every_24_pulses() {
        let interval_ms = 60_000.0 / (120.0 * 24.0);
        let mut sync = MidiClockSync::new();
        let results = feed_pulses(&mut sync, 0.0, interval_ms, 48);

        for (i, (_, beat_fired)) in results.iter().enumerate() {
            let pulse = i as u32 + 1;
            assert_eq!(
                *beat_fired,
                pulse.is_multiple_of(24),
                "pulse {pulse}: beat_fired was {beat_fired}"
            );
        }
    }

    #[test]
    fn out_of_range_bpm_is_omitted_not_clamped() {
        // Intervals implying ~1000 BPM: 60000 / (1000 * 24) = 2.5ms apart.
        let interval_ms = 60_000.0 / (1000.0 * 24.0);
        let mut sync = MidiClockSync::new();
        let results = feed_pulses(&mut sync, 0.0, interval_ms, 12);

        for (i, (bpm, _)) in results.iter().enumerate() {
            let pulse = i as u32 + 1;
            if pulse.is_multiple_of(6) && pulse >= 7 {
                assert_eq!(*bpm, None, "pulse {pulse}: expected no bpm, got {bpm:?}");
            }
        }
    }

    #[test]
    fn no_bpm_before_seven_samples_available() {
        let interval_ms = 60_000.0 / (120.0 * 24.0);
        let mut sync = MidiClockSync::new();
        // Only 6 pulses fed: pulse 6 is a multiple of 6, but only 6 samples
        // are in the ring (< 7 needed), so bpm must be None.
        let results = feed_pulses(&mut sync, 0.0, interval_ms, 6);
        assert_eq!(results[5].0, None);
    }

    #[test]
    fn on_timeout_resets_internal_state() {
        let interval_ms = 60_000.0 / (120.0 * 24.0);
        let mut sync = MidiClockSync::new();
        feed_pulses(&mut sync, 0.0, interval_ms, 10);

        sync.on_timeout();

        // After a reset, the pulse count restarts, so beat_fired should not
        // fire again until 24 fresh pulses have been fed.
        let results = feed_pulses(&mut sync, 1_000_000.0, interval_ms, 23);
        assert!(results.iter().all(|(_, beat_fired)| !beat_fired));
        let (_, beat_fired) = sync.on_pulse(1_000_000.0 + interval_ms * 23.0);
        assert!(beat_fired);
    }

    #[test]
    fn ring_buffer_caps_at_49_and_drops_oldest() {
        let interval_ms = 60_000.0 / (120.0 * 24.0);
        let mut sync = MidiClockSync::new();
        // Feed far more than 49 pulses; bpm should keep converging on 120.0
        // using only the most recent samples, proving the ring dropped the
        // oldest entries rather than growing unbounded or using stale data.
        let results = feed_pulses(&mut sync, 0.0, interval_ms, 120);
        let (bpm, _) = results.last().unwrap();
        assert!((bpm.unwrap() - 120.0).abs() <= 0.1);
    }
}
