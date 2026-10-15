use super::super::*;
use crate::commands::{create_default_registry, CommandId};

mod beat_sync_defaults {
    use super::*;

    #[test]
    fn beat_sync_starts_false_for_both_decks() {
        let show = Show::default();
        assert!(!show.beat_sync_a);
        assert!(!show.beat_sync_b);
    }

    #[test]
    fn beat_trigger_configs_start_at_default() {
        let show = Show::default();
        assert_eq!(show.beat_trigger_a, default_beat_trigger_config());
        assert_eq!(show.beat_trigger_b, default_beat_trigger_config());
    }

    #[test]
    fn auto_xfade_starts_false_with_beats_per_change_8() {
        let show = Show::default();
        assert!(!show.auto_xfade);
        assert_eq!(show.beats_per_change, 8);
    }

    #[test]
    fn manual_bpm_starts_at_zero() {
        assert_eq!(Show::default().manual_bpm, 0.0);
    }
}

mod on_beat {
    use super::*;

    fn catalog(names: &[&str]) -> Vec<PresetMeta> {
        names.iter().map(|n| PresetMeta { name: n.to_string(), category: "Other".to_string() }).collect()
    }

    fn every_beat_trigger() -> BeatTriggerConfig {
        BeatTriggerConfig { mode: crate::beat_trigger::BeatTriggerMode::Beat, beats_per_change: 1, offset: 0, sensitivity: 0.5 }
    }

    #[test]
    fn beat_sync_advances_a_bus_unless_the_autopilot_owns_it() {
        let make = |autopilot_a: bool| Show {
            beat_sync_a: true,
            beat_trigger_a: every_beat_trigger(),
            preset_catalog: catalog(&["a", "b", "c"]),
            autopilot_a,
            ..Default::default()
        };

        let mut plain = make(false);
        plain.on_beat();
        assert!(!plain.take_fired_presets().is_empty(), "beat sync advances a bus on its own");

        let mut piloted = make(true);
        piloted.on_beat();
        assert!(piloted.take_fired_presets().is_empty(), "the auto-pilot replaces the beat advance");
    }

    #[test]
    fn the_playlist_timer_stands_down_for_a_piloted_bus() {
        let fired_after_a_minute = |autopilot_a: bool| {
            let mut show = Show { autopilot_a, ..Default::default() };
            show.playlists.interval_sec = 1.0;
            show.playlists.add_to_playlist(Deck::A, "x".to_string());
            show.playlists.add_to_playlist(Deck::A, "y".to_string());
            show.playlists.toggle_playlist(Deck::A);
            show.tick_playlists(0.0); // applies the interval for this state
            let _ = show.take_fired_presets(); // anything fired by starting the playlist
            show.tick_playlists(60_000.0);
            show.take_fired_presets().len()
        };
        assert!(fired_after_a_minute(false) > 0, "a playing playlist advances on its own timer");
        assert_eq!(fired_after_a_minute(true), 0, "no timer advance while the auto-pilot owns the bus");
    }

    #[test]
    fn auto_xfade_does_not_toggle_before_the_configured_beat_count() {
        let mut show = Show { auto_xfade: true, beats_per_change: 4, ..Default::default() };
        for _ in 0..3 {
            show.on_beat();
            assert_eq!(show.crossfader, 0.0);
        }
    }

    #[test]
    fn auto_xfade_toggles_on_the_nth_beat() {
        let mut show = Show { auto_xfade: true, beats_per_change: 4, ..Default::default() };
        for _ in 0..4 {
            show.on_beat();
        }
        assert_eq!(show.crossfader, 1.0);
    }

    #[test]
    fn auto_xfade_toggles_back_after_another_n_beats() {
        let mut show = Show { auto_xfade: true, beats_per_change: 2, ..Default::default() };
        show.on_beat();
        show.on_beat();
        assert_eq!(show.crossfader, 1.0);
        show.on_beat();
        show.on_beat();
        assert_eq!(show.crossfader, 0.0);
    }

    #[test]
    fn reset_auto_xfade_count_restarts_the_cadence_from_the_top() {
        // Without a reset, re-enabling auto-xfade mid-count would fire
        // the next crossfade early.
        let mut show = Show { auto_xfade: true, beats_per_change: 4, ..Default::default() };
        show.on_beat();
        show.on_beat();
        show.on_beat(); // 3 beats in, one short of the 4-beat cadence
        assert_eq!(show.crossfader, 0.0);
        show.reset_auto_xfade_count();
        show.on_beat(); // would have flipped here without the reset
        assert_eq!(show.crossfader, 0.0);
        show.on_beat();
        show.on_beat();
        show.on_beat();
        assert_eq!(show.crossfader, 1.0); // flips a full 4 beats after the reset
    }

    #[test]
    fn does_not_touch_crossfader_when_auto_xfade_is_off() {
        let mut show = Show { beats_per_change: 1, ..Default::default() };
        show.on_beat();
        assert_eq!(show.crossfader, 0.0);
    }

    #[test]
    fn does_not_advance_when_beat_sync_is_false() {
        let mut show = Show { preset_catalog: catalog(&["P0", "P1"]), ..Default::default() };
        // beat_sync_a/b default to false.
        show.on_beat();
        assert!(show.fired_preset_a.borrow().is_none());
        assert!(show.fired_preset_b.borrow().is_none());
    }

    #[test]
    fn does_not_advance_when_locked_even_if_beat_sync_is_true() {
        let mut show = Show { preset_catalog: catalog(&["P0", "P1"]), beat_sync_a: true, lock_a: true, ..Default::default() };
        show.on_beat();
        assert!(show.fired_preset_a.borrow().is_none());
    }

    #[test]
    fn advances_the_playlist_when_it_has_items() {
        let mut show = Show::default();
        show.playlists.add_to_playlist(Deck::A, "p1".to_string());
        show.playlists.add_to_playlist(Deck::A, "p2".to_string());
        show.beat_sync_a = true;
        show.on_beat();
        assert_eq!(show.playlists.engine_a_mut().unwrap().current_index(), 1);
    }

    #[test]
    fn navigates_the_preset_catalog_when_the_playlist_is_empty() {
        let mut show = Show { preset_catalog: catalog(&["P0", "P1"]), beat_sync_a: true, ..Default::default() };
        show.on_beat();
        assert_eq!(show.fired_preset_a.borrow().as_deref(), Some("P1"));
    }

    #[test]
    fn deck_b_advances_independently_of_deck_a() {
        let mut show = Show::default();
        show.playlists.add_to_playlist(Deck::B, "p1".to_string());
        show.playlists.add_to_playlist(Deck::B, "p2".to_string());
        show.beat_sync_b = true;
        show.on_beat();
        assert_eq!(show.playlists.engine_b_mut().unwrap().current_index(), 1);
        assert!(show.fired_preset_a.borrow().is_none());
    }

    #[test]
    fn respects_the_configured_trigger_cadence_via_the_clock_beat_count() {
        let mut show = Show { preset_catalog: catalog(&["P0", "P1"]), beat_sync_a: true, ..Default::default() };
        show.beat_trigger_a.beats_per_change = 4;
        show.clock.pulse(None); // beat_count = 1
        show.on_beat();
        show.clock.pulse(None); // 2
        show.on_beat();
        show.clock.pulse(None); // 3
        show.on_beat();
        assert!(show.fired_preset_a.borrow().is_none());
        show.clock.pulse(None); // 4
        show.on_beat();
        assert_eq!(show.fired_preset_a.borrow().as_deref(), Some("P1"));
    }
}

mod toggle_beat_sync {
    use super::*;

    #[test]
    fn sets_the_playlist_interval_to_infinity_then_restores_it() {
        let mut show = Show::default();
        show.playlists.add_to_playlist(Deck::A, "p1".to_string());
        show.playlists.interval_sec = 5.0;

        show.toggle_beat_sync(Deck::A);
        assert!(show.beat_sync_a);
        assert_eq!(show.playlists.engine_a_mut().unwrap().interval_ms(), f64::INFINITY);

        show.toggle_beat_sync(Deck::A);
        assert!(!show.beat_sync_a);
        assert_eq!(show.playlists.engine_a_mut().unwrap().interval_ms(), 5000.0);
    }
}

mod tap_tempo {
    use super::*;

    #[test]
    fn a_single_tap_does_not_set_a_bpm() {
        let mut show = Show::default();
        show.tap_tempo(0.0);
        assert_eq!(show.manual_bpm, 0.0);
    }

    #[test]
    fn computes_bpm_from_a_regular_interval() {
        let mut show = Show::default();
        show.tap_tempo(0.0);
        show.tap_tempo(500.0); // 500ms -> 120bpm
        assert_eq!(show.manual_bpm, 120.0);
        assert_eq!(show.clock.bpm(), 120.0);
    }

    #[test]
    fn averages_over_all_kept_intervals() {
        let mut show = Show::default();
        show.tap_tempo(0.0);
        show.tap_tempo(400.0); // interval 400
        show.tap_tempo(1000.0); // interval 600, avg 500 -> 120bpm
        assert_eq!(show.manual_bpm, 120.0);
    }

    #[test]
    fn rejects_a_computed_bpm_below_40() {
        let mut show = Show::default();
        show.tap_tempo(0.0);
        show.tap_tempo(3000.0); // 3000ms -> 20bpm
        assert_eq!(show.manual_bpm, 0.0);
    }

    #[test]
    fn rejects_a_computed_bpm_above_300() {
        let mut show = Show::default();
        show.tap_tempo(0.0);
        show.tap_tempo(100.0); // 100ms -> 600bpm
        assert_eq!(show.manual_bpm, 0.0);
    }

    #[test]
    fn keeps_only_the_last_4_taps() {
        let mut show = Show::default();
        show.tap_tempo(0.0);
        show.tap_tempo(500.0);
        show.tap_tempo(1000.0);
        show.tap_tempo(1500.0);
        show.tap_tempo(2000.0); // 5th tap: oldest (0.0) is dropped
        assert_eq!(show.tap_times, vec![500.0, 1000.0, 1500.0, 2000.0]);
    }
}

mod clear_manual_bpm {
    use super::*;

    #[test]
    fn resets_manual_bpm_tap_times_and_clock_bpm() {
        let mut show = Show::default();
        show.tap_tempo(0.0);
        show.tap_tempo(500.0);
        assert_eq!(show.manual_bpm, 120.0);

        show.clear_manual_bpm();
        assert_eq!(show.manual_bpm, 0.0);
        assert!(show.tap_times.is_empty());
        assert_eq!(show.clock.bpm(), 0.0);
    }
}

mod current_bpm {
    use super::*;

    fn commit_detected_bpm(show: &mut Show) {
        for i in 0..43 {
            show.beat_detector.process_sample(10.0, i as f64 * 10.0);
        }
        let mut now = 10_000.0;
        show.beat_detector.process_sample(50.0, now);
        now += 310.0;
        show.beat_detector.process_sample(50.0, now);
        now += 310.0;
        show.beat_detector.process_sample(50.0, now); // commits bpm = 194.0
    }

    #[test]
    fn returns_zero_when_neither_manual_nor_detected_bpm_is_set() {
        assert_eq!(Show::default().current_bpm(), 0.0);
    }

    #[test]
    fn falls_back_to_the_detected_bpm_when_no_manual_bpm_is_set() {
        let mut show = Show::default();
        commit_detected_bpm(&mut show);
        assert_eq!(show.current_bpm(), 194.0);
    }

    #[test]
    fn manual_bpm_takes_priority_over_the_detected_bpm() {
        let mut show = Show::default();
        commit_detected_bpm(&mut show);
        show.manual_bpm = 128.0;
        assert_eq!(show.current_bpm(), 128.0);
    }
}

mod check_volume_peak_triggers {
    use super::*;

    fn catalog(names: &[&str]) -> Vec<PresetMeta> {
        names.iter().map(|n| PresetMeta { name: n.to_string(), category: "Other".to_string() }).collect()
    }

    fn volume_peak_trigger() -> BeatTriggerConfig {
        BeatTriggerConfig { mode: BeatTriggerMode::VolumePeak, beats_per_change: 8, offset: 0, sensitivity: 0.5 }
    }

    #[test]
    fn does_nothing_in_beat_mode() {
        let mut show = Show { preset_catalog: catalog(&["P0", "P1"]), beat_sync_a: true, ..Default::default() }; // default trigger mode is Beat
        show.check_volume_peak_triggers(0.9, 1000.0);
        assert!(show.fired_preset_a.borrow().is_none());
    }

    #[test]
    fn does_nothing_when_not_synced() {
        let mut show = Show { preset_catalog: catalog(&["P0", "P1"]), beat_trigger_a: volume_peak_trigger(), ..Default::default() };
        // beat_sync_a defaults to false.
        show.check_volume_peak_triggers(0.9, 1000.0);
        assert!(show.fired_preset_a.borrow().is_none());
    }

    #[test]
    fn does_nothing_when_locked() {
        let mut show = Show { preset_catalog: catalog(&["P0", "P1"]), beat_trigger_a: volume_peak_trigger(), beat_sync_a: true, lock_a: true, ..Default::default() };
        show.check_volume_peak_triggers(0.9, 1000.0);
        assert!(show.fired_preset_a.borrow().is_none());
    }

    #[test]
    fn advances_on_a_clear_peak_in_volume_peak_mode() {
        let mut show = Show { preset_catalog: catalog(&["P0", "P1"]), beat_trigger_a: volume_peak_trigger(), beat_sync_a: true, ..Default::default() };
        show.check_volume_peak_triggers(0.2, 1000.0); // establishes the rolling average
        show.check_volume_peak_triggers(0.9, 1600.0); // clear peak above it
        assert_eq!(show.fired_preset_a.borrow().as_deref(), Some("P1"));
    }

    #[test]
    fn processes_both_decks_independently() {
        let mut show = Show { preset_catalog: catalog(&["P0", "P1"]), beat_trigger_a: volume_peak_trigger(), beat_trigger_b: volume_peak_trigger(), beat_sync_a: true, beat_sync_b: true, ..Default::default() };
        show.check_volume_peak_triggers(0.2, 1000.0);
        show.check_volume_peak_triggers(0.9, 1600.0);
        assert_eq!(show.fired_preset_a.borrow().as_deref(), Some("P1"));
        assert_eq!(show.fired_preset_b.borrow().as_deref(), Some("P1"));
    }
}

/// `Show`'s half of the video-clip cut.
/// The cut *rules* themselves are covered in `core::video`; what's
/// tested here is only the seam: that `Show` feeds the layer a real
/// draw, and that the draw is reseedable like every other RNG consumer.
mod on_video_beat {
    use super::*;

    fn keys(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("clip{i}")).collect()
    }

    #[test]
    fn a_disabled_video_layer_never_cuts() {
        let mut show = Show::default();
        assert!(!show.video.enabled);
        assert!(!show.on_video_beat(&keys(4), false));
    }

    #[test]
    fn an_enabled_layer_cuts_once_per_beats_per_cut_beats() {
        let mut show = Show::default();
        show.video.enabled = true;
        show.video.beats_per_cut = 2;
        show.video.advance = crate::video::VideoAdvance::Sequential;
        let all = keys(3);
        assert!(!show.on_video_beat(&all, false));
        assert!(show.on_video_beat(&all, false));
        assert_eq!(show.video.current_clip_index, 1);
    }

    #[test]
    fn shuffle_draws_differ_across_seeds() {
        let draws = |seed: u64| -> Vec<usize> {
            let mut show = Show::default();
            show.video.enabled = true;
            show.video.beats_per_cut = 1;
            show.reseed_rng(seed);
            let all = keys(16);
            (0..12)
                .map(|_| {
                    show.on_video_beat(&all, false);
                    show.video.current_clip_index
                })
                .collect()
        };
        assert_ne!(draws(1), draws(2));
    }

    #[test]
    fn the_same_seed_reproduces_the_same_shuffle_sequence() {
        let draws = || -> Vec<usize> {
            let mut show = Show::default();
            show.video.enabled = true;
            show.video.beats_per_cut = 1;
            show.reseed_rng(99);
            let all = keys(16);
            (0..12)
                .map(|_| {
                    show.on_video_beat(&all, false);
                    show.video.current_clip_index
                })
                .collect()
        };
        assert_eq!(draws(), draws());
    }
}

/// `on_deck_video_beat` must touch only the targeted slot's
/// `VideoState`, leaving the other 3 deck slots and the global `video`
/// layer untouched.
mod on_deck_video_beat {
    use super::*;

    fn keys(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("clip{i}")).collect()
    }

    #[test]
    fn cuts_only_the_targeted_slot_leaving_others_untouched() {
        let mut show = Show::default();
        for slot in 0..4 {
            show.deck_video[slot].enabled = true;
            show.deck_video[slot].beats_per_cut = 1;
            show.deck_video[slot].advance = crate::video::VideoAdvance::Sequential;
        }
        show.video.enabled = true;
        show.video.beats_per_cut = 1;
        show.video.advance = crate::video::VideoAdvance::Sequential;
        let all = keys(3);

        assert!(show.on_deck_video_beat(1, &all));

        assert_eq!(show.deck_video[1].current_clip_index, 1);
        assert_eq!(show.deck_video[0].current_clip_index, 0);
        assert_eq!(show.deck_video[2].current_clip_index, 0);
        assert_eq!(show.deck_video[3].current_clip_index, 0);
        assert_eq!(show.video.current_clip_index, 0);
    }
}

mod reseed_rng {
    use super::*;

    #[test]
    fn makes_deck_a_shuffle_draws_actually_differ_across_seeds() {
        let draws = |seed: u64| -> Vec<usize> {
            let mut show = Show::default();
            for name in ["p1", "p2", "p3", "p4", "p5"] {
                show.playlists.add_to_playlist(Deck::A, name.to_string());
            }
            show.playlists.engine_a_mut().unwrap().set_mode(PlaylistMode::Shuffle);
            show.reseed_rng(seed);
            (0..10)
                .map(|_| {
                    show.playlist_next(Deck::A);
                    show.playlists.engine_a_mut().unwrap().current_index()
                })
                .collect()
        };
        assert_ne!(draws(1), draws(2));
    }

    #[test]
    fn seeds_deck_a_and_deck_b_differently_so_they_do_not_lockstep() {
        let mut show = Show::default();
        for name in ["p1", "p2", "p3", "p4", "p5"] {
            show.playlists.add_to_playlist(Deck::A, name.to_string());
            show.playlists.add_to_playlist(Deck::B, name.to_string());
        }
        show.playlists.engine_a_mut().unwrap().set_mode(PlaylistMode::Shuffle);
        show.playlists.engine_b_mut().unwrap().set_mode(PlaylistMode::Shuffle);
        show.reseed_rng(42);
        let draws_a: Vec<usize> = (0..10)
            .map(|_| {
                show.playlist_next(Deck::A);
                show.playlists.engine_a_mut().unwrap().current_index()
            })
            .collect();
        let draws_b: Vec<usize> = (0..10)
            .map(|_| {
                show.playlist_next(Deck::B);
                show.playlists.engine_b_mut().unwrap().current_index()
            })
            .collect();
        assert_ne!(draws_a, draws_b);
    }

    #[test]
    fn seeds_the_4_deck_video_rng_slots_distinctly() {
        let mut show = Show::default();
        show.reseed_rng(7);
        let draws: Vec<f64> = (0..4).map(|slot| show.deck_video_rng[slot].next_f64()).collect();
        assert_ne!(draws[0], draws[1]);
    }
}

/// Minimum bar: an LFO must actually reach at least one Color,
/// one Composite, one Time, and one Qvar target end to end: tick the
/// engine, dispatch the resulting `(CommandId, value01)` through the
/// real registry (same path `app::about_to_wait` uses), assert `Show`
/// changed. `amount: 0.0` pins `LfoEngine::compute`'s output to
/// `center` regardless of phase/shape/S&H state (`value = center +
/// (raw - 0.5) * amount`), deterministic without depending on a
/// specific shape's waveform, which `lfo.rs`'s own tests already cover.
mod lfo_end_to_end_dispatch {
    use super::*;
    use crate::lfo::LfoSlot;

    fn slot(center: f64, target: CommandId) -> LfoSlot {
        LfoSlot { enabled: true, center, amount: 0.0, target: Some(target), ..Default::default() }
    }

    fn tick_and_dispatch(reg: &crate::commands::CommandRegistry, show: &mut Show) {
        let outputs = show.lfo_engine.tick(show.clock.phase01());
        for output in outputs {
            if let Some(id) = output.target {
                reg.dispatch(id, output.value01, show);
            }
        }
    }

    #[test]
    fn reaches_a_color_target() {
        let reg = create_default_registry();
        let mut show = Show::default();
        show.lfo_engine.slots[0] = slot(0.6, CommandId::ColorHueA);

        tick_and_dispatch(&reg, &mut show);

        assert_eq!(show.color_params_a.hue_rotate, 0.6);
    }

    #[test]
    fn reaches_a_composite_target() {
        let reg = create_default_registry();
        let mut show = Show::default();
        show.lfo_engine.slots[0] = slot(0.9, CommandId::CompositeBlend0);

        tick_and_dispatch(&reg, &mut show);

        assert_eq!(show.slot_composites[0].blend, blend_mode_from_value01(0.9));
    }

    #[test]
    fn reaches_a_time_target() {
        let reg = create_default_registry();
        let mut show = Show::default();
        show.lfo_engine.slots[0] = slot(1.0, CommandId::TimeSpeed0);

        tick_and_dispatch(&reg, &mut show);

        assert_eq!(show.time_params[0].speed_mult, crate::time_params::TIME_MULT_MAX);
    }

    #[test]
    fn reaches_a_qvar_target() {
        let reg = create_default_registry();
        let mut show = Show::default();
        show.lfo_engine.slots[0] = slot(1.0, CommandId::Qvar1_0);

        tick_and_dispatch(&reg, &mut show);

        assert!(show.q_var_params[0].enabled[0]);
        assert_eq!(show.q_var_params[0].value[0], crate::q_vars::Q_VAR_MAX);
    }

    #[test]
    fn a_disabled_slot_dispatches_nothing() {
        let reg = create_default_registry();
        let mut show = Show::default();
        show.lfo_engine.slots[0] = LfoSlot { enabled: false, target: Some(CommandId::ColorHueA), ..Default::default() };

        tick_and_dispatch(&reg, &mut show);

        assert_eq!(show.color_params_a.hue_rotate, 0.0); // untouched default
    }
}
