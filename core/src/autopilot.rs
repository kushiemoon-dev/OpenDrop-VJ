//! The energy auto-pilot: every so many beats it chooses the next preset for
//! a bus so that how lively the preset looks follows how lively the music
//! is. Off by default and per bus; nothing here loads a preset, it only
//! decides which one and when.
//!
//! Pure data and decisions: `app` owns the scan results, feeds the audio
//! energy in and performs the load.

use crate::rng::Xorshift64;

/// Beats between two automatic changes unless the user picks another value.
pub const DEFAULT_INTERVAL_BEATS: u32 = 32;
pub const MIN_INTERVAL_BEATS: u32 = 4;
pub const MAX_INTERVAL_BEATS: u32 = 256;
/// How many of the closest-matching presets the choice is drawn from, so the
/// same loud passage does not always select the same preset.
const SHORTLIST: usize = 5;

/// Time constants for the music-energy follower, in seconds.
const LEVEL_SMOOTHING_SECONDS: f32 = 6.0;
const PEAK_DECAY_SECONDS: f32 = 90.0;
/// The reference level never drops below this, so near-silence does not read
/// as maximum energy.
const PEAK_FLOOR: f32 = 0.15;

/// Follows the music's energy and reports it relative to how loud this
/// source has been lately, so a quiet recording still reaches its high end.
#[derive(Debug, Clone, Copy, Default)]
pub struct EnergyTracker {
    smoothed: f32,
    peak: f32,
}

impl EnergyTracker {
    /// `energy` is the raw level in 0 to 1, `dt_seconds` the time since the
    /// previous call.
    pub fn update(&mut self, energy: f32, dt_seconds: f32) {
        let energy = energy.clamp(0.0, 1.0);
        let dt = dt_seconds.clamp(0.0, 1.0);
        self.smoothed += (energy - self.smoothed) * (1.0 - (-dt / LEVEL_SMOOTHING_SECONDS).exp());
        let decayed = self.peak * (-dt / PEAK_DECAY_SECONDS).exp();
        self.peak = decayed.max(self.smoothed);
    }

    /// Current energy, 0 (quiet) to 1 (as loud as it has been lately).
    pub fn level(&self) -> f32 {
        (self.smoothed / self.peak.max(PEAK_FLOOR)).clamp(0.0, 1.0)
    }
}

#[derive(Debug, Clone)]
pub struct Autopilot {
    pub enabled_a: bool,
    pub enabled_b: bool,
    interval_beats: u32,
    since_a: u32,
    since_b: u32,
    rng: Xorshift64,
}

impl Default for Autopilot {
    fn default() -> Self {
        Self {
            enabled_a: false,
            enabled_b: false,
            interval_beats: DEFAULT_INTERVAL_BEATS,
            since_a: 0,
            since_b: 0,
            rng: Xorshift64::default(),
        }
    }
}

impl Autopilot {
    pub fn interval_beats(&self) -> u32 {
        self.interval_beats
    }

    pub fn set_interval_beats(&mut self, beats: u32) {
        self.interval_beats = beats.clamp(MIN_INTERVAL_BEATS, MAX_INTERVAL_BEATS);
    }

    pub fn reseed(&mut self, seed: u64) {
        self.rng.reseed(seed);
    }

    /// Counts `beats` and returns which buses are due a change, `(a, b)`.
    /// A bus that is switched off neither counts nor fires, so turning it on
    /// starts a fresh interval.
    pub fn on_beats(&mut self, beats: u32) -> (bool, bool) {
        let a = Self::advance(self.enabled_a, &mut self.since_a, beats, self.interval_beats);
        let b = Self::advance(self.enabled_b, &mut self.since_b, beats, self.interval_beats);
        (a, b)
    }

    fn advance(enabled: bool, since: &mut u32, beats: u32, interval: u32) -> bool {
        if !enabled {
            *since = 0;
            return false;
        }
        *since += beats;
        if *since >= interval {
            *since = 0;
            true
        } else {
            false
        }
    }

    /// Chooses a preset whose energy is close to `target` (0 to 1) from
    /// `candidates` (name and energy), leaving out `avoid` unless that would
    /// leave nothing. `None` only when there are no candidates at all.
    pub fn choose(&mut self, candidates: &[(&str, f32)], target: f32, avoid: &[&str]) -> Option<String> {
        let mut pool: Vec<&(&str, f32)> = candidates.iter().filter(|(name, _)| !avoid.contains(name)).collect();
        if pool.is_empty() {
            pool = candidates.iter().collect();
        }
        pool.sort_by(|a, b| (a.1 - target).abs().total_cmp(&(b.1 - target).abs()));
        let shortlist = &pool[..pool.len().min(SHORTLIST)];
        if shortlist.is_empty() {
            return None;
        }
        let pick = ((self.rng.next_f64() * shortlist.len() as f64) as usize).min(shortlist.len() - 1);
        Some(shortlist[pick].0.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tracker_rises_with_loud_music_and_falls_when_it_quiets() {
        let mut tracker = EnergyTracker::default();
        for _ in 0..600 {
            tracker.update(0.9, 0.1);
        }
        assert!(tracker.level() > 0.9, "level was {}", tracker.level());
        for _ in 0..600 {
            tracker.update(0.1, 0.1);
        }
        assert!(tracker.level() < 0.4, "level was {}", tracker.level());
    }

    #[test]
    fn a_quiet_source_still_reaches_a_high_level_relative_to_itself() {
        let mut tracker = EnergyTracker::default();
        for _ in 0..600 {
            tracker.update(0.3, 0.1);
        }
        assert!(tracker.level() > 0.9, "level was {}", tracker.level());
    }

    #[test]
    fn silence_reads_as_low_energy_not_as_maximum() {
        let mut tracker = EnergyTracker::default();
        for _ in 0..600 {
            tracker.update(0.01, 0.1);
        }
        assert!(tracker.level() < 0.2, "level was {}", tracker.level());
    }

    #[test]
    fn only_enabled_buses_count_and_fire_at_the_interval() {
        let mut pilot = Autopilot::default();
        pilot.set_interval_beats(8);
        assert_eq!(pilot.on_beats(100), (false, false), "everything off: nothing fires");

        pilot.enabled_a = true;
        assert_eq!(pilot.on_beats(5), (false, false));
        assert_eq!(pilot.on_beats(3), (true, false));
        assert_eq!(pilot.on_beats(7), (false, false), "the count restarts after firing");

        pilot.enabled_a = false;
        pilot.enabled_b = true;
        assert_eq!(pilot.on_beats(8), (false, true));
    }

    #[test]
    fn the_interval_is_clamped() {
        let mut pilot = Autopilot::default();
        pilot.set_interval_beats(0);
        assert_eq!(pilot.interval_beats(), MIN_INTERVAL_BEATS);
        pilot.set_interval_beats(10_000);
        assert_eq!(pilot.interval_beats(), MAX_INTERVAL_BEATS);
    }

    #[test]
    fn the_choice_stays_close_to_the_target_and_skips_recent_presets() {
        let candidates: Vec<(&str, f32)> =
            vec![("calm", 0.1), ("mid1", 0.45), ("mid2", 0.5), ("mid3", 0.55), ("wild", 0.95), ("wilder", 1.0), ("mid4", 0.6)];
        let mut pilot = Autopilot::default();
        for seed in 0..50 {
            pilot.reseed(seed);
            let chosen = pilot.choose(&candidates, 0.5, &["mid2"]).unwrap();
            assert!(["mid1", "mid3", "mid4", "calm", "wild"].contains(&chosen.as_str()), "chose {chosen}");
            assert_ne!(chosen, "mid2");
            assert_ne!(chosen, "wilder", "far from the target");
        }
    }

    #[test]
    fn avoiding_everything_falls_back_to_all_and_no_candidates_gives_none() {
        let mut pilot = Autopilot::default();
        assert_eq!(pilot.choose(&[("only", 0.5)], 0.5, &["only"]).as_deref(), Some("only"));
        assert_eq!(pilot.choose(&[], 0.5, &[]), None);
    }
}
