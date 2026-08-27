//! Port of OpenDrop-VJ `src/lib/engine/beat-trigger.ts`: playlist beat/volume
//! trigger config: fixed-interval beat triggering, or an alternative
//! volume-peak trigger, driving playlist A/B advance timing.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeatTriggerMode {
    Beat,
    VolumePeak,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BeatTriggerConfig {
    pub mode: BeatTriggerMode,
    /// 1..64
    pub beats_per_change: u32,
    /// 0..beats_per_change-1
    pub offset: u32,
    /// 0..1, used only in `BeatTriggerMode::VolumePeak`
    pub sensitivity: f64,
}

pub fn default_beat_trigger_config() -> BeatTriggerConfig {
    BeatTriggerConfig { mode: BeatTriggerMode::Beat, beats_per_change: 8, offset: 0, sensitivity: 0.5 }
}

pub fn should_trigger_on_beat(beat_count: i64, config: BeatTriggerConfig) -> bool {
    if config.mode != BeatTriggerMode::Beat {
        return false;
    }
    (beat_count + config.offset as i64) % config.beats_per_change as i64 == 0
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VolumePeakState {
    pub rolling_avg: f64,
    /// ms timestamp, for cooldown
    pub last_trigger_at: f64,
}

pub fn default_volume_peak_state() -> VolumePeakState {
    // Mirrors the JS `-Infinity`: the very first peak is never blocked by cooldown.
    VolumePeakState { rolling_avg: 0.0, last_trigger_at: f64::NEG_INFINITY }
}

const COOLDOWN_MS: f64 = 500.0;
const SMOOTHING: f64 = 0.05;
const SILENCE_FLOOR: f64 = 0.02;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VolumePeakResult {
    pub triggered: bool,
    pub next: VolumePeakState,
}

pub fn detect_volume_peak(rms: f64, state: VolumePeakState, sensitivity: f64, now_ms: f64) -> VolumePeakResult {
    let next_avg = state.rolling_avg * (1.0 - SMOOTHING) + rms * SMOOTHING;
    let threshold_mult = 1.3 + sensitivity * 1.7;
    let cooled_down = now_ms - state.last_trigger_at >= COOLDOWN_MS;
    let triggered = cooled_down && next_avg > SILENCE_FLOOR && rms > next_avg * threshold_mult;
    VolumePeakResult {
        triggered,
        next: VolumePeakState {
            rolling_avg: next_avg,
            last_trigger_at: if triggered { now_ms } else { state.last_trigger_at },
        },
    }
}

pub fn clamp_beats_per_change(n: i64) -> u32 {
    n.clamp(1, 64) as u32
}

pub fn clamp_offset(offset: i64, beats_per_change: u32) -> u32 {
    offset.clamp(0, beats_per_change as i64 - 1) as u32
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BeatTriggerConfigPatch {
    pub mode: Option<BeatTriggerMode>,
    pub beats_per_change: Option<i64>,
    pub offset: Option<i64>,
    pub sensitivity: Option<f64>,
}

/// Merges `patch` into `current`, re-clamping `beats_per_change`/`offset` so
/// they stay valid together: `offset` is clamped against the NEW
/// `beats_per_change`, not the one `current` had before the patch.
pub fn apply_beat_trigger_patch(current: BeatTriggerConfig, patch: BeatTriggerConfigPatch) -> BeatTriggerConfig {
    let beats_per_change = clamp_beats_per_change(patch.beats_per_change.unwrap_or(current.beats_per_change as i64));
    let offset = clamp_offset(patch.offset.unwrap_or(current.offset as i64), beats_per_change);
    BeatTriggerConfig {
        mode: patch.mode.unwrap_or(current.mode),
        beats_per_change,
        offset,
        sensitivity: patch.sensitivity.unwrap_or(current.sensitivity),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_beat_trigger_config_returns_mode_beat_threshold_8_offset_0_sensitivity_0_5() {
        assert_eq!(
            default_beat_trigger_config(),
            BeatTriggerConfig { mode: BeatTriggerMode::Beat, beats_per_change: 8, offset: 0, sensitivity: 0.5 }
        );
    }

    mod should_trigger_on_beat {
        use super::*;

        #[test]
        fn triggers_every_n_beats() {
            let cfg = BeatTriggerConfig { mode: BeatTriggerMode::Beat, beats_per_change: 4, offset: 0, sensitivity: 0.5 };
            assert!(should_trigger_on_beat(0, cfg));
            assert!(!should_trigger_on_beat(1, cfg));
            assert!(should_trigger_on_beat(4, cfg));
            assert!(should_trigger_on_beat(8, cfg));
        }

        #[test]
        fn respects_the_offset() {
            let cfg = BeatTriggerConfig { mode: BeatTriggerMode::Beat, beats_per_change: 4, offset: 2, sensitivity: 0.5 };
            assert!(!should_trigger_on_beat(0, cfg));
            assert!(should_trigger_on_beat(2, cfg));
            assert!(should_trigger_on_beat(6, cfg));
        }

        #[test]
        fn never_triggers_in_volume_peak_mode() {
            let cfg = BeatTriggerConfig { mode: BeatTriggerMode::VolumePeak, beats_per_change: 4, offset: 0, sensitivity: 0.5 };
            assert!(!should_trigger_on_beat(0, cfg));
            assert!(!should_trigger_on_beat(4, cfg));
        }
    }

    mod detect_volume_peak_tests {
        use super::*;

        #[test]
        fn does_not_trigger_below_the_threshold() {
            let state = VolumePeakState { rolling_avg: 0.3, last_trigger_at: f64::NEG_INFINITY };
            assert!(!detect_volume_peak(0.35, state, 0.5, 1000.0).triggered);
        }

        #[test]
        fn triggers_on_a_clear_peak_above_the_rolling_average() {
            let state = VolumePeakState { rolling_avg: 0.2, last_trigger_at: f64::NEG_INFINITY };
            assert!(detect_volume_peak(0.9, state, 0.5, 1000.0).triggered);
        }

        #[test]
        fn respects_the_cooldown_no_retrigger_before_500ms() {
            let state = VolumePeakState { rolling_avg: 0.2, last_trigger_at: 1000.0 };
            assert!(!detect_volume_peak(0.9, state, 0.5, 1300.0).triggered);
        }

        #[test]
        fn re_triggers_after_the_cooldown() {
            let state = VolumePeakState { rolling_avg: 0.2, last_trigger_at: 1000.0 };
            assert!(detect_volume_peak(0.9, state, 0.5, 1600.0).triggered);
        }

        #[test]
        fn ignores_near_silence_even_with_a_high_ratio() {
            let state = VolumePeakState { rolling_avg: 0.005, last_trigger_at: f64::NEG_INFINITY };
            assert!(!detect_volume_peak(0.019, state, 1.0, 1000.0).triggered);
        }

        #[test]
        fn the_rolling_average_follows_an_increasing_volume_trend() {
            let mut state = default_volume_peak_state();
            for i in 0..50 {
                state = detect_volume_peak(0.5, state, 0.5, i as f64 * 100.0).next;
            }
            assert!(state.rolling_avg > 0.4);
        }

        #[test]
        fn updates_last_trigger_at_only_when_it_triggers() {
            let state = VolumePeakState { rolling_avg: 0.3, last_trigger_at: f64::NEG_INFINITY };
            let result = detect_volume_peak(0.35, state, 0.5, 1000.0);
            assert_eq!(result.next.last_trigger_at, f64::NEG_INFINITY);
        }
    }

    mod clamp_beats_per_change_tests {
        use super::*;

        #[test]
        fn clamps_between_1_and_64() {
            assert_eq!(clamp_beats_per_change(0), 1);
            assert_eq!(clamp_beats_per_change(100), 64);
            assert_eq!(clamp_beats_per_change(8), 8);
        }
    }

    mod clamp_offset_tests {
        use super::*;

        #[test]
        fn clamps_between_0_and_beats_per_change_minus_1() {
            assert_eq!(clamp_offset(-1, 8), 0);
            assert_eq!(clamp_offset(10, 8), 7);
            assert_eq!(clamp_offset(3, 8), 3);
        }
    }

    mod apply_beat_trigger_patch_tests {
        use super::*;

        #[test]
        fn merges_a_partial_patch_without_touching_fields_that_werent_provided() {
            let current = default_beat_trigger_config();
            let next = apply_beat_trigger_patch(
                current,
                BeatTriggerConfigPatch { mode: Some(BeatTriggerMode::VolumePeak), ..Default::default() },
            );
            assert_eq!(next.mode, BeatTriggerMode::VolumePeak);
            assert_eq!(next.beats_per_change, 8);
            assert_eq!(next.sensitivity, 0.5);
        }

        #[test]
        fn re_clamps_beats_per_change_after_the_patch() {
            let current = default_beat_trigger_config();
            assert_eq!(
                apply_beat_trigger_patch(current, BeatTriggerConfigPatch { beats_per_change: Some(100), ..Default::default() })
                    .beats_per_change,
                64
            );
            assert_eq!(
                apply_beat_trigger_patch(current, BeatTriggerConfigPatch { beats_per_change: Some(0), ..Default::default() })
                    .beats_per_change,
                1
            );
        }

        #[test]
        fn re_clamps_offset_relative_to_the_new_beats_per_change_not_the_old_one() {
            let current = BeatTriggerConfig { beats_per_change: 8, offset: 7, ..default_beat_trigger_config() };
            let next = apply_beat_trigger_patch(
                current,
                BeatTriggerConfigPatch { beats_per_change: Some(4), ..Default::default() },
            );
            assert_eq!(next.offset, 3);
        }

        #[test]
        fn does_not_mutate_the_current_object() {
            let current = default_beat_trigger_config();
            let next = apply_beat_trigger_patch(
                current,
                BeatTriggerConfigPatch { mode: Some(BeatTriggerMode::VolumePeak), ..Default::default() },
            );
            assert_eq!(current.mode, BeatTriggerMode::Beat);
            assert_ne!(next, current);
        }
    }
}
