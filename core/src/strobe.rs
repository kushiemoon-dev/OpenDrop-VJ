//! Strobe flash timing (Step 10 of the Phase 8 VJ-panels plan): a
//! BPM-synced full-screen flash, rendered by the compositor
//! (`engine::compositor::Compositor::render_strobe_flash`). No OpenDrop-VJ
//! source to port from beyond the command name: `SidebarStrobe.svelte`
//! only held on/off/rate/intensity/color state, the timing itself is new
//! here, following the same colocated-test convention as
//! `snapshot.rs`/`timeline.rs`/`q_vars.rs`/`lfo.rs`.

/// User-facing strobe state (Strobe panel: `app::ui::strobe`). `rate` is a
/// multiplier of beat rate, same convention as `lfo::LfoSlot::rate`: 1 =
/// once per beat, 2 = twice per beat, 0.5 = once every 2 beats. The panel's
/// rate buttons only ever assign one of {0.25, 0.5, 1, 2, 4}, but nothing
/// here enforces that set: `strobe_flash_intensity` works for any
/// positive `rate`. `intensity` is 0..1 (the panel's slider range); `color`
/// is straight RGB in 0..1, sampled as-is by the compositor's
/// additive-blend pass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrobeState {
    pub enabled: bool,
    pub rate: f64,
    pub intensity: f64,
    pub color: [f32; 3],
}

impl Default for StrobeState {
    fn default() -> Self {
        Self { enabled: false, rate: 1.0, intensity: 0.5, color: [1.0, 1.0, 1.0] }
    }
}

/// How long a single flash takes to decay from peak to 0, in seconds:
/// capped (see `strobe_flash_intensity`) to half the current sub-beat
/// period so a fast rate/high-bpm combination never holds one flash into
/// the start of the next.
const FLASH_DECAY_SEC: f64 = 0.08;

/// Per-frame flash intensity: 0 (no flash) up to `state.intensity` (peak,
/// right at a trigger instant), decaying linearly to 0 over
/// [`FLASH_DECAY_SEC`]. `enabled == false` or a non-positive `rate` is
/// always 0.
///
/// **Tempo present** (`clock_bpm > 0`): triggers are the multiples of
/// `rate` in `clock_beats_abs`, the ABSOLUTE number of beats elapsed since
/// the clock started (`Clock::beat_count() as f64 + Clock::phase01()`, not
/// `phase01()` alone). `phase01` alone is only the fractional position
/// *within* the current beat, always in `[0, 1)`: for `rate < 1` that is
/// not enough information: a `rate == 0.25` flash (once every 4 beats)
/// needs to know *which* beat this is, not just where in it we are, or
/// every `rate` below 1 collapses to "once per beat" (the bug a code
/// review caught: reducing `phase01 * beat_period` modulo a sub-period
/// longer than one beat is a no-op, since that value never reaches the
/// sub-period in the first place). Multiplying the absolute beat count by
/// `rate` first, then taking the fractional part, is what makes a
/// `rate == 0.25` cycle span 4 real beats and a `rate == 2` cycle span
/// half of one. `now_sec` is unused on this path: `clock_beats_abs` is the
/// synced source of truth for where the beat is (MIDI clock / Ableton Link
/// / the audio beat detector all drive it, see `Show::clock`), independent
/// of this process's own uptime.
///
/// **No tempo** (`clock_bpm <= 0`, e.g. nothing detected/set yet: see
/// `Show::current_bpm`): `Clock::phase01`/`beat_count` are frozen
/// (`Clock::step` never advances either at bpm 0), so there is no beat to
/// sync a flash to. Falls back to a free-running pulse at `rate` Hz,
/// driven by `now_sec` instead, so toggling Strobe on still does
/// something visible before a tempo locks.
pub fn strobe_flash_intensity(state: &StrobeState, clock_beats_abs: f64, clock_bpm: f64, now_sec: f64) -> f32 {
    if !state.enabled || state.rate <= 0.0 {
        return 0.0;
    }
    let sub_period_sec = if clock_bpm > 0.0 { 60.0 / (clock_bpm * state.rate) } else { 1.0 / state.rate };
    let time_since_edge_sec = if clock_bpm > 0.0 {
        (clock_beats_abs * state.rate).rem_euclid(1.0) * sub_period_sec
    } else {
        now_sec.rem_euclid(sub_period_sec)
    };
    let flash_len_sec = FLASH_DECAY_SEC.min(sub_period_sec * 0.5);
    if time_since_edge_sec >= flash_len_sec {
        return 0.0;
    }
    let decay = 1.0 - time_since_edge_sec / flash_len_sec;
    (state.intensity.clamp(0.0, 1.0) * decay) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled(rate: f64, intensity: f64) -> StrobeState {
        StrobeState { enabled: true, rate, intensity, color: [1.0, 1.0, 1.0] }
    }

    /// Counts flash-start events (a 0 -> >0 transition) across `beats`
    /// beats of `clock_beats_abs`, sampled at `steps_per_beat` steps per
    /// beat, over the half-open range `[0, beats)`: the same numerical
    /// method a code review used to catch the original rate < 1 bug (60fps
    /// @ 120 BPM ~ 30 steps/beat). Regression coverage for every one of
    /// the panel's 5 rate buttons, not just the ones that happened to work
    /// before the fix.
    fn count_flash_starts(state: &StrobeState, bpm: f64, beats: u32, steps_per_beat: u32) -> u32 {
        let total_steps = beats * steps_per_beat;
        let mut starts = 0;
        let mut was_off = true;
        for i in 0..total_steps {
            let beats_abs = i as f64 / steps_per_beat as f64;
            let is_on = strobe_flash_intensity(state, beats_abs, bpm, 0.0) > 0.0;
            if is_on && was_off {
                starts += 1;
            }
            was_off = !is_on;
        }
        starts
    }

    mod disabled_and_invalid_rate {
        use super::*;

        #[test]
        fn disabled_is_always_0_regardless_of_phase_or_bpm() {
            let state = StrobeState { enabled: false, ..enabled(1.0, 1.0) };
            assert_eq!(strobe_flash_intensity(&state, 0.0, 120.0, 0.0), 0.0);
        }

        #[test]
        fn zero_rate_is_always_0() {
            let state = enabled(0.0, 1.0);
            assert_eq!(strobe_flash_intensity(&state, 0.0, 120.0, 0.0), 0.0);
        }

        #[test]
        fn negative_rate_is_always_0() {
            let state = enabled(-1.0, 1.0);
            assert_eq!(strobe_flash_intensity(&state, 0.0, 120.0, 0.0), 0.0);
        }
    }

    mod tempo_synced {
        use super::*;

        // bpm=120 -> beat period 0.5s; rate=1 -> sub-period 0.5s, flash_len
        // = min(0.08, 0.25) = 0.08s. `clock_beats_abs` below equals
        // `phase01` exactly since these all stay within beat 0
        // (beat_count implicitly 0).

        #[test]
        fn peaks_at_state_intensity_right_on_the_beat() {
            let state = enabled(1.0, 0.8);
            assert_eq!(strobe_flash_intensity(&state, 0.0, 120.0, 0.0), 0.8);
        }

        #[test]
        fn decays_linearly_toward_0() {
            let state = enabled(1.0, 0.8);
            // time_since_edge = 0.04s (half of the 0.08s flash) ->
            // beats_abs = 0.04 / 0.5 = 0.08.
            let v = strobe_flash_intensity(&state, 0.08, 120.0, 0.0);
            assert!((v - 0.4).abs() < 1e-6, "{v}");
        }

        #[test]
        fn reaches_exactly_0_at_the_flash_length_boundary() {
            let state = enabled(1.0, 0.8);
            // time_since_edge = 0.08s -> beats_abs = 0.08 / 0.5 = 0.16.
            assert_eq!(strobe_flash_intensity(&state, 0.16, 120.0, 0.0), 0.0);
        }

        #[test]
        fn stays_0_through_the_rest_of_the_beat() {
            let state = enabled(1.0, 0.8);
            assert_eq!(strobe_flash_intensity(&state, 0.5, 120.0, 0.0), 0.0);
            assert_eq!(strobe_flash_intensity(&state, 0.99, 120.0, 0.0), 0.0);
        }

        #[test]
        fn rate_2_adds_a_second_trigger_mid_beat() {
            let state = enabled(2.0, 0.8);
            // sub-period = 60/(120*2) = 0.25s -> edges at beats_abs 0.0 and 0.5.
            assert_eq!(strobe_flash_intensity(&state, 0.5, 120.0, 0.0), 0.8);
            // Halfway between edges (beats_abs 0.25) is well past the 0.08s
            // flash window (time_since_edge = 0.125s).
            assert_eq!(strobe_flash_intensity(&state, 0.25, 120.0, 0.0), 0.0);
        }

        #[test]
        fn flash_length_is_capped_to_half_the_sub_period_at_a_fast_rate() {
            // bpm=300, rate=4 -> sub-period = 60/(300*4) = 0.05s, half of
            // that (0.025s) is less than FLASH_DECAY_SEC (0.08s), so the
            // flash must not bleed past 0.025s into the next one.
            let state = enabled(4.0, 1.0);
            // time_since_edge = 0.01s -> beats_abs = 0.01 / (60/300) = 0.05.
            let v = strobe_flash_intensity(&state, 0.05, 300.0, 0.0);
            assert!((v - 0.6).abs() < 1e-6, "{v}"); // decay = 1 - 0.01/0.025
            // time_since_edge = 0.025s (the capped boundary) -> beats_abs = 0.125.
            assert_eq!(strobe_flash_intensity(&state, 0.125, 300.0, 0.0), 0.0);
        }

        /// Regression coverage for the rate < 1 bug a code review found:
        /// `phase01`-only reduction made 0.25/0.5/1 all trigger once per
        /// beat, indistinguishably. Counts flash starts over 4 beats at
        /// 120 BPM for all 5 of the panel's rate buttons; expected counts
        /// are exactly what a correct "N triggers per beat" (`rate >= 1`)
        /// or "one trigger every 1/rate beats" (`rate < 1`) reading gives,
        /// and exactly what the code review's own manual simulation
        /// found.
        mod flash_start_count_over_4_beats_at_120_bpm {
            use super::*;

            const BEATS: u32 = 4;
            const STEPS_PER_BEAT: u32 = 30; // ~60fps at 120 BPM (2 beats/sec).
            const BPM: f64 = 120.0;

            #[test]
            fn rate_0_25_flashes_once_every_4_beats() {
                let state = enabled(0.25, 1.0);
                assert_eq!(count_flash_starts(&state, BPM, BEATS, STEPS_PER_BEAT), 1);
            }

            #[test]
            fn rate_0_5_flashes_once_every_2_beats() {
                let state = enabled(0.5, 1.0);
                assert_eq!(count_flash_starts(&state, BPM, BEATS, STEPS_PER_BEAT), 2);
            }

            #[test]
            fn rate_1_flashes_once_per_beat() {
                let state = enabled(1.0, 1.0);
                assert_eq!(count_flash_starts(&state, BPM, BEATS, STEPS_PER_BEAT), 4);
            }

            #[test]
            fn rate_2_flashes_twice_per_beat() {
                let state = enabled(2.0, 1.0);
                assert_eq!(count_flash_starts(&state, BPM, BEATS, STEPS_PER_BEAT), 8);
            }

            #[test]
            fn rate_4_flashes_4_times_per_beat() {
                let state = enabled(4.0, 1.0);
                assert_eq!(count_flash_starts(&state, BPM, BEATS, STEPS_PER_BEAT), 16);
            }
        }
    }

    mod free_running_without_tempo {
        use super::*;

        #[test]
        fn peaks_at_now_sec_0() {
            let state = enabled(2.0, 0.8);
            assert_eq!(strobe_flash_intensity(&state, 0.0, 0.0, 0.0), 0.8);
        }

        #[test]
        fn decays_over_the_same_flash_window() {
            let state = enabled(2.0, 0.8);
            // sub-period = 1/2 = 0.5s, flash_len = min(0.08, 0.25) = 0.08s.
            let v = strobe_flash_intensity(&state, 0.0, 0.0, 0.04);
            assert!((v - 0.4).abs() < 1e-6, "{v}");
            assert_eq!(strobe_flash_intensity(&state, 0.0, 0.0, 0.3), 0.0);
        }

        #[test]
        fn wraps_back_to_peak_on_the_next_cycle() {
            let state = enabled(2.0, 0.8);
            assert_eq!(strobe_flash_intensity(&state, 0.0, 0.0, 0.5), 0.8);
        }

        #[test]
        fn bpm_exactly_0_uses_the_free_running_path() {
            // Guards the `> 0.0` (not `!= 0.0`) branch condition explicitly.
            let state = enabled(1.0, 1.0);
            assert_eq!(strobe_flash_intensity(&state, 0.0, 0.0, 0.0), 1.0);
        }
    }

    mod intensity_scaling {
        use super::*;

        #[test]
        fn peak_scales_with_state_intensity_not_fixed_at_1() {
            let state = enabled(1.0, 0.3);
            assert_eq!(strobe_flash_intensity(&state, 0.0, 120.0, 0.0), 0.3);
        }

        #[test]
        fn out_of_range_intensity_is_clamped_to_0_1() {
            let state = enabled(1.0, 5.0);
            assert_eq!(strobe_flash_intensity(&state, 0.0, 120.0, 0.0), 1.0);
            let state = enabled(1.0, -5.0);
            assert_eq!(strobe_flash_intensity(&state, 0.0, 120.0, 0.0), 0.0);
        }
    }
}
