use opendrop_core::blend::DEFAULT_COLOR_PARAMS;
use opendrop_core::show::{DeckBus, Show};
use opendrop_engine::deck::{self};
use std::sync::Arc;
use std::time::{Duration, Instant};
use crate::frame::autopilot::run_autopilot;
use crate::AppState;
use opendrop_audio::AudioSnapshot;
use opendrop_engine::compositor::LayerInput;

/// Real (measured) dt for
/// `Show::clock.step`/`Show::tick_playlists`, instead of the nominal
/// `refresh_interval` the render-pacing loop targets. `about_to_wait`'s
/// pacing resyncs `next_frame_at = now + refresh_interval` whenever the
/// loop falls behind, discarding the overrun (see that resync branch's own
/// comment). Feeding nominal dt into the beat-sync clock silently
/// under-counted exactly the time a tick that fell behind actually took.
/// At 4 decks + egui + dual-window blit genuinely running at 40fps against
/// a 60fps target, a 128 BPM beat-sync played back around 85 BPM.
///
/// `last_tick_at` is `None` only on the very first tick, before there is
/// anything to measure against yet, where nominal dt is the only sane
/// fallback. `max_dt` matches the TS reference's `Math.min(dt, 0.1)`
/// (`clock.ts:53`, "avoid large jumps on tab-visibility change"); the same
/// rationale covers a debugger pause or any other long stall here, so a
/// real one doesn't feed a huge jump into beat-sync.
///
/// Frame-pacing itself (the `WaitUntil(next_frame_at)` scheduling) is
/// unaffected: it keeps targeting `refresh_interval` as before. Only the
/// clock/playlist tick inputs change to real elapsed time. Pure, so it's
/// testable without a real event loop.
pub(crate) fn compute_tick_dt(now: Instant, last_tick_at: Option<Instant>, nominal: Duration, max_dt: Duration) -> f64 {
    let elapsed = match last_tick_at {
        Some(prev) => now.saturating_duration_since(prev),
        None => nominal,
    };
    elapsed.min(max_dt).as_secs_f64()
}

/// Clamp applied to `compute_tick_dt`'s real elapsed-time measurement,
/// matching the TS reference's `Math.min(dt, 0.1)` (`clock.ts:53`).
const MAX_TICK_DT: Duration = Duration::from_millis(100);

/// Per-slot compositor input driven by the live show state: opacity from
/// `bus_gain(deck_bus[slot], crossfader)`, composite config directly from
/// `slot_composites`, and color params from whichever bus (A/B) that slot is
/// currently assigned to. `Off` slots get the default (harmless, since
/// their opacity is 0 and composite_layer skips them at the 0.001 floor).
pub(crate) fn layer_inputs_from_show(show: &Show) -> [LayerInput; 4] {
    let opacities = show.slot_opacities();
    std::array::from_fn(|i| {
        let color = match show.deck_bus[i] {
            DeckBus::A => show.color_params_a,
            DeckBus::B => show.color_params_b,
            DeckBus::Off => DEFAULT_COLOR_PARAMS,
        };
        LayerInput { opacity: opacities[i] as f32, composite: show.slot_composites[i], color }
    })
}

/// What one tick hands to the render stages that follow it.
pub(crate) struct TickOutput {
    pub(crate) layer_inputs: [LayerInput; 4],
    pub(crate) audio: Arc<AudioSnapshot>,
    pub(crate) now_ms: f64,
    pub(crate) ndi_active: bool,
}

/// Advances the show by one tick: beat clock and sync, playlists, snapshot recall,
/// timeline, LFOs and the video layer's audio reaction.
pub(crate) fn tick_show(state: &mut AppState, now: Instant) -> TickOutput {
    let layer_inputs = layer_inputs_from_show(&state.show);

    // Each deck context injects one PCM chunk, renders one projectM
    // frame, and copies it into its shared texture; then, back on
    // the main context, each texture is drawn through the
    // compositor shader into the composite FBO. A deck at or below
    // the 0.001 opacity floor, never sampled by composite_layer
    // either way, is culled down to IDLE_DECK_INTERVAL instead of
    // rendering at full rate for nothing: the "4 decks rendered at
    // full resolution even while invisible" pathology from the
    // diagnostic, killed at the root rather than papered over.
    let audio = state.audio.latest();

    // Beat-sync engine wiring: runs once per tick, before
    // the deck-rendering loop below. `take_fired_presets` is drained
    // at the top of `about_to_wait`, once per call, before this
    // render-gated block: a preset/playlist advance fired by
    // `show.on_beat()` here becomes visible on that drain at the
    // start of the next `about_to_wait` call, same path as a
    // keyboard-driven `navigate_preset`. Placing this block ahead of
    // the deck-rendering loop, rather than after it, starts that
    // preset's async pre-flight validation as early as possible in
    // the tick. `now_ms` is `t0.elapsed()`, a monotonic clock
    // independent of `next_frame_at` (the render-pacing deadline,
    // which drifts by design, see the resync branch below). Never
    // derive it from `next_frame_at`.
    let now_ms = state.t0.elapsed().as_secs_f64() * 1000.0;
    // Real elapsed time, not the nominal refresh_interval; see
    // `compute_tick_dt`'s doc comment.
    let dt = compute_tick_dt(now, state.last_output_swap_at, state.refresh_interval, MAX_TICK_DT);
    // `beats_this_tick` feeds the beat-reactive overlay pulse below: the native
    // `beatSyncState.beat`. Counted at the same two `on_beat()`
    // call sites, so it reflects exactly the beats the engine
    // acted on, not an independent re-derivation. It is a count rather than a bool because the
    // video layer's clip cut is a
    // per-beat *counter* (`beats_per_cut`), so a tick that carries
    // 2 beats has to advance it twice; a bool would silently slow
    // the cut cadence down whenever the tick rate dips below the
    // beat rate.
    let mut beats_this_tick = 0u32;
    if state.show.manual_bpm == 0.0 {
        let r = state.show.beat_detector.process_sample(audio.energy_byte, now_ms);
        if r.beat_triggered {
            state.show.clock.pulse(Some(r.bpm));
            if state.show.clock.bpm() == 0.0 {
                state.show.on_beat();
                beats_this_tick += 1;
            }
        }
    }
    for _ in 0..state.show.clock.step(dt) {
        state.show.on_beat();
        beats_this_tick += 1;
    }
    let beat_fired = beats_this_tick > 0;
    state.energy_tracker.update((audio.energy_byte / 255.0) as f32, dt as f32);
    run_autopilot(state, beats_this_tick);
    if beat_fired {
        state.last_beat_at = Some(now);
    }
    // Video layer. Two
    // per-tick hooks, both ports of the web's own:
    //  - the beat-driven clip cut, called here rather than from
    //    `Show::on_beat` because the clip list it rotates through
    //    is `app`-owned (files on disk),
    //  - the bass-driven speed warp, just below, next to the VU
    //    reading it consumes.
    // Neither one starts or stops anything itself: both only move
    // `Show::video`, and the single reconciliation step further
    // down turns whatever they changed into at most one
    // Start/Stop. `ndi_active` is read once here and reused by
    // both, the reconciliation, and the composite pass.
    let ndi_active = state.ndi.latest().receive_active;
    if beat_fired {
        let clip_keys: Vec<String> = state.video_clips.iter().map(|c| c.key.clone()).collect();
        // Once per beat, not once per tick; see `beats_this_tick`.
        // The return value (did the clip actually change?) is not
        // needed here: the reconciliation below compares the
        // resolved `VideoInput`, which carries the clip's path, so
        // a shuffle redraw of the clip already playing correctly
        // restarts nothing.
        for _ in 0..beats_this_tick {
            state.show.on_video_beat(&clip_keys, ndi_active);
            for slot in 0..deck::DECK_COUNT {
                state.show.on_deck_video_beat(slot, &clip_keys);
            }
        }
    }
    // The interval-driven half of the same playlist engines the
    // beats above drive: without this the Playlists panel's
    // "Interval (s)" slider does nothing and Play only ever loads
    // the current item. Same `dt` as `clock.step`, converted to the
    // milliseconds `PlaylistEngine` works in.
    state.show.tick_playlists(dt * 1000.0);
    // Snapshot recall. Same
    // dt-driven cadence as `tick_playlists` just above: `Show` has
    // no wall clock of its own (see `Show::reseed_rng`'s doc
    // comment), so `tick_recall` accumulates elapsed time from this
    // same `dt` rather than an `Instant`. Dispatching the returned
    // pairs through `state.registry`, instead of writing
    // `state.show`'s fields directly, keeps a recall on the exact
    // same command path as keyboard/MIDI/OSC/remote-ws, only
    // `CommandId`s with a real `CommandContext` setter move visibly,
    // which is all but two commands (`LfoRateUp`/
    // `LfoRateDown` are the permanent exceptions). Note this is a
    // separate question from what a snapshot *captures*: see
    // `SNAPSHOT_CAPTURABLE_IDS` in `core::show` for the deliberate
    // Time/Qvar exclusion there.
    for (id, value) in state.show.tick_recall(dt) {
        state.registry.dispatch(id, value, &mut state.show);
    }
    // Timeline playback. Same dt-driven cadence and
    // registry-dispatch parity as the
    // snapshot recall loop just above.
    for (id, value) in state.show.tick_timeline(dt) {
        state.registry.dispatch(id, value, &mut state.show);
    }
    // LFO modulation. Same
    // registry-dispatch parity as the snapshot recall/timeline
    // loops above, but driven by beat phase rather than dt:
    // `LfoSlot::rate` is a multiplier of beat rate (see
    // `core::lfo`'s doc comments), not Hz, so it reads
    // `state.show.clock.phase01()` directly instead of
    // accumulating its own elapsed time from `dt`.
    let lfo_outputs = state.show.lfo_engine.tick(state.show.clock.phase01());
    for output in lfo_outputs {
        if let Some(id) = output.target {
            state.registry.dispatch(id, output.value01, &mut state.show);
        }
    }
    state.last_vu_level = opendrop_audio::analysis::vu_level(&audio.pcm);
    state.show.check_volume_peak_triggers(state.last_vu_level, now_ms);
    // `onVideoAudioTick(lv.bass)`, `+page.svelte:563`, the same
    // call site, right after the volume-peak triggers. The web's
    // `bass` is the mean of the bass FFT bins divided by 255;
    // `AudioSnapshot::energy_byte` is the RMS of those same bins
    // still in byte units (`audio::analysis::bass_energy`), so the
    // /255 here puts it on the same 0..1 scale.
    let previous_rate = state.show.video.playback_rate;
    state.show.video.on_audio_tick(audio.energy_byte / 255.0, ndi_active);
    // Only on a real change: this is a cross-thread store, and the
    // rate is otherwise pinned (warp off) or moving by fractions of
    // a percent per tick.
    if (state.show.video.playback_rate - previous_rate).abs() > 0.001 {
        let _ = state
            .video
            .control_tx
            .send(opendrop_io::video_capture::VideoCaptureControl::SetRate(state.show.video.playback_rate));
    }
    // Per-deck audio warp: always `ndi_active: false`
    // (`on_deck_video_beat` already commits to this; see
    // its doc comment), since a deck-video slot has no NDI receive
    // path of its own to defer to.
    for slot in 0..deck::DECK_COUNT {
        let previous_rate = state.show.deck_video[slot].playback_rate;
        state.show.deck_video[slot].on_audio_tick(audio.energy_byte / 255.0, false);
        if (state.show.deck_video[slot].playback_rate - previous_rate).abs() > 0.001 {
            let _ = state.deck_video_capture[slot].control_tx.send(
                opendrop_io::video_capture::VideoCaptureControl::SetRate(state.show.deck_video[slot].playback_rate),
            );
        }
    }
    TickOutput { layer_inputs, audio, now_ms, ndi_active }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real elapsed-time dt for beat-sync/
    /// playlist ticking.
    mod compute_tick_dt_tests {
        use super::*;

        #[test]
        fn first_tick_uses_nominal_dt() {
            let now = Instant::now();
            let dt = compute_tick_dt(now, None, Duration::from_millis(16), Duration::from_millis(100));
            assert!((dt - 0.016).abs() < 1e-6);
        }

        #[test]
        fn measures_real_elapsed_time_not_nominal() {
            let prev = Instant::now();
            let now = prev + Duration::from_millis(25);
            // Nominal would be 16ms (60fps), but the tick actually took
            // 25ms (fell behind); the real, measured value has to win.
            let dt = compute_tick_dt(now, Some(prev), Duration::from_millis(16), Duration::from_millis(100));
            assert!((dt - 0.025).abs() < 1e-6);
        }

        #[test]
        fn clamps_a_long_stall_to_max_dt() {
            let prev = Instant::now();
            let now = prev + Duration::from_secs(2);
            let dt = compute_tick_dt(now, Some(prev), Duration::from_millis(16), Duration::from_millis(100));
            assert_eq!(dt, 0.1);
        }

        #[test]
        fn exactly_at_the_clamp_boundary_is_not_reduced() {
            let prev = Instant::now();
            let now = prev + Duration::from_millis(100);
            let dt = compute_tick_dt(now, Some(prev), Duration::from_millis(16), Duration::from_millis(100));
            assert_eq!(dt, 0.1);
        }
    }
}
