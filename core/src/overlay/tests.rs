use super::*;

mod make_overlay_tests {
    use super::*;

    #[test]
    fn defaults_to_kind_media_backward_compatible() {
        let ov = make_overlay("id-1".to_string(), "mon-image".to_string(), OverlayPatch::default());
        assert_eq!(ov.kind, OverlayKind::Media);
        assert!(!ov.video);
        assert_eq!(ov.text, "");
    }

    #[test]
    fn creates_a_text_overlay_with_the_correct_defaults() {
        let ov = make_overlay(
            "id-1".to_string(),
            "Text".to_string(),
            OverlayPatch {
                kind: Some(OverlayKind::Text),
                text: Some("Hello".to_string()),
                ..Default::default()
            },
        );
        assert_eq!(ov.kind, OverlayKind::Text);
        assert_eq!(ov.text, "Hello");
        assert_eq!(ov.font_family, FontFamily::Sans);
        assert_eq!(ov.font_size, 8.0);
        assert_eq!(ov.color, "#ffffff");
    }

    #[test]
    fn shared_fields_remain_unchanged() {
        let ov = make_overlay(
            "id-1".to_string(),
            "Text".to_string(),
            OverlayPatch { kind: Some(OverlayKind::Text), ..Default::default() },
        );
        assert_eq!(ov.x, 0.5);
        assert_eq!(ov.y, 0.5);
        assert_eq!(ov.scale, 1.0);
        assert_eq!(ov.opacity, 1.0);
        assert_eq!(ov.spin, 0.0);
        assert_eq!(ov.drift_x, 0.0);
        assert_eq!(ov.drift_y, 0.0);
    }

    #[test]
    fn partial_override_replaces_only_the_provided_fields() {
        let ov = make_overlay(
            "id-1".to_string(),
            "Text".to_string(),
            OverlayPatch {
                kind: Some(OverlayKind::Text),
                font_family: Some(FontFamily::Impact),
                color: Some("#ff2d78".to_string()),
                ..Default::default()
            },
        );
        assert_eq!(ov.font_family, FontFamily::Impact);
        assert_eq!(ov.color, "#ff2d78");
        assert_eq!(ov.font_size, 8.0);
    }

    #[test]
    fn in_queue_defaults_to_false() {
        let ov = make_overlay("id-1".to_string(), "mon-image".to_string(), OverlayPatch::default());
        assert!(!ov.in_queue);
    }
}

mod overlay_store_tests {
    use super::*;

    #[test]
    fn add_text_overlay_adds_a_text_overlay_and_returns_its_id() {
        let mut store = OverlayStore::new();
        let id = store.add_text_overlay("id-1".to_string());
        assert_eq!(store.overlays.len(), 1);
        assert_eq!(store.overlays[0].kind, OverlayKind::Text);
        assert_eq!(store.overlays[0].id, id);
    }

    #[test]
    fn add_overlay_at_position_adds_a_positioned_overlay() {
        let mut store = OverlayStore::new();
        store.add_overlay_at_position("id-1".to_string(), "photo".to_string(), 0.2, 0.8);
        assert_eq!(store.overlays.len(), 1);
        assert_eq!(store.overlays[0].x, 0.2);
        assert_eq!(store.overlays[0].y, 0.8);
    }

    #[test]
    fn remove_overlay_deletes_the_overlay_from_the_list() {
        let mut store = OverlayStore::new();
        let id = store.add_text_overlay("id-1".to_string());
        store.remove_overlay(&id);
        assert!(store.overlays.is_empty());
    }

    #[test]
    fn remove_overlay_re_clamps_queue_index_if_the_queue_shrinks() {
        let mut store = OverlayStore::new();
        store.overlays = vec![
            Overlay { id: "a".to_string(), in_queue: true, ..Default::default() },
            Overlay { id: "b".to_string(), in_queue: true, ..Default::default() },
        ];
        store.queue_index = 1;
        store.remove_overlay("b");
        assert_eq!(store.queue_index, 0);
    }

    #[test]
    fn update_overlay_merges_a_patch_onto_the_correct_overlay_only() {
        let mut store = OverlayStore::new();
        let id = store.add_text_overlay("id-1".to_string());
        store.update_overlay(&id, OverlayPatch { opacity: Some(0.4), ..Default::default() });
        assert_eq!(store.overlays[0].opacity, 0.4);
        assert_eq!(store.overlays[0].text, "Text");
    }

    #[test]
    fn toggle_overlay_queue_toggles_enabled() {
        let mut store = OverlayStore::new();
        assert!(!store.queue_enabled);
        store.toggle_overlay_queue();
        assert!(store.queue_enabled);
        store.toggle_overlay_queue();
        assert!(!store.queue_enabled);
    }

    #[test]
    fn set_overlay_queue_mode_replaces_the_mode() {
        let mut store = OverlayStore::new();
        store.set_overlay_queue_mode(PlaylistMode::Shuffle);
        assert_eq!(store.queue_mode, PlaylistMode::Shuffle);
    }

    #[test]
    fn update_overlay_queue_trigger_merges_and_re_clamps_via_apply_beat_trigger_patch() {
        let mut store = OverlayStore::new();
        store.update_overlay_queue_trigger(BeatTriggerConfigPatch {
            beats_per_change: Some(100),
            ..Default::default()
        });
        assert_eq!(store.queue_trigger.beats_per_change, 64);
    }

    #[test]
    fn advance_overlay_queue_advances_sequentially_among_the_queued_overlays() {
        let mut store = OverlayStore::new();
        store.overlays = vec![
            Overlay { id: "a".to_string(), in_queue: true, ..Default::default() },
            Overlay { id: "b".to_string(), in_queue: true, ..Default::default() },
            Overlay { id: "c".to_string(), in_queue: false, ..Default::default() },
        ];
        store.queue_mode = PlaylistMode::Sequential;
        store.queue_index = 0;
        store.advance_overlay_queue(1);
        assert_eq!(store.queue_index, 1);
    }

    #[test]
    fn advance_overlay_queue_goes_backward_regardless_of_mode() {
        let mut store = OverlayStore::new();
        store.overlays = vec![
            Overlay { id: "a".to_string(), in_queue: true, ..Default::default() },
            Overlay { id: "b".to_string(), in_queue: true, ..Default::default() },
        ];
        store.queue_index = 1;
        store.advance_overlay_queue(-1);
        assert_eq!(store.queue_index, 0);
    }

    #[test]
    fn advance_overlay_queue_shuffle_never_repeats_the_current_index() {
        let mut store = OverlayStore::new();
        store.overlays = vec![
            Overlay { id: "a".to_string(), in_queue: true, ..Default::default() },
            Overlay { id: "b".to_string(), in_queue: true, ..Default::default() },
            Overlay { id: "c".to_string(), in_queue: true, ..Default::default() },
        ];
        store.queue_mode = PlaylistMode::Shuffle;
        let mut prev = store.queue_index;
        for _ in 0..30 {
            store.advance_overlay_queue(1);
            assert_ne!(store.queue_index, prev);
            prev = store.queue_index;
        }
    }

    #[test]
    fn advance_overlay_queue_shuffle_with_a_single_queued_overlay_always_picks_index_0() {
        let mut store = OverlayStore::new();
        store.overlays = vec![Overlay { id: "only".to_string(), in_queue: true, ..Default::default() }];
        store.queue_mode = PlaylistMode::Shuffle;
        for _ in 0..5 {
            store.advance_overlay_queue(1);
        }
        assert_eq!(store.queue_index, 0);
    }

    #[test]
    fn on_visualizer_drag_over_activates_drag_over_only_when_files_are_dragged() {
        let mut store = OverlayStore::new();
        let should_prevent_default = store.on_visualizer_drag_over(true);
        assert!(store.drag_over);
        assert!(should_prevent_default);
    }

    #[test]
    fn on_visualizer_drag_over_ignores_a_dragover_without_files() {
        let mut store = OverlayStore::new();
        let should_prevent_default = store.on_visualizer_drag_over(false);
        assert!(!store.drag_over);
        assert!(!should_prevent_default);
    }

    #[test]
    fn reseed_rng_makes_shuffle_draws_actually_differ_across_seeds() {
        let mut store = OverlayStore::new();
        store.overlays = vec![
            Overlay { id: "a".to_string(), in_queue: true, ..Default::default() },
            Overlay { id: "b".to_string(), in_queue: true, ..Default::default() },
            Overlay { id: "c".to_string(), in_queue: true, ..Default::default() },
            Overlay { id: "d".to_string(), in_queue: true, ..Default::default() },
            Overlay { id: "e".to_string(), in_queue: true, ..Default::default() },
        ];
        store.queue_mode = PlaylistMode::Shuffle;
        let draws = |store: &mut OverlayStore, seed: u64| -> Vec<usize> {
            store.reseed_rng(seed);
            store.queue_index = 0;
            (0..10)
                .map(|_| {
                    store.advance_overlay_queue(1);
                    store.queue_index
                })
                .collect()
        };
        let seq_1 = draws(&mut store, 1);
        let seq_2 = draws(&mut store, 2);
        assert_ne!(seq_1, seq_2);
    }
}

mod overlay_queue_trigger_tests {
    use super::*;

    fn store_with_two_queued() -> OverlayStore {
        let mut store = OverlayStore::new();
        store.overlays = vec![
            Overlay { id: "a".to_string(), in_queue: true, ..Default::default() },
            Overlay { id: "b".to_string(), in_queue: true, ..Default::default() },
        ];
        store
    }

    #[test]
    fn beat_trigger_does_nothing_while_the_queue_is_disabled() {
        let mut store = store_with_two_queued();
        store.maybe_advance_on_beat(0);
        store.maybe_advance_on_beat(8);
        assert_eq!(store.queue_index, 0);
    }

    #[test]
    fn beat_trigger_advances_every_beats_per_change_beats() {
        let mut store = store_with_two_queued();
        store.queue_enabled = true;
        // default beats_per_change is 8, offset 0
        for beat in 0..8 {
            store.maybe_advance_on_beat(beat);
        }
        // only beat 0 matched
        assert_eq!(store.queue_index, 1);
        store.maybe_advance_on_beat(8);
        assert_eq!(store.queue_index, 0);
    }

    #[test]
    fn beat_trigger_never_fires_while_the_trigger_is_in_volume_peak_mode() {
        let mut store = store_with_two_queued();
        store.queue_enabled = true;
        store.update_overlay_queue_trigger(BeatTriggerConfigPatch {
            mode: Some(BeatTriggerMode::VolumePeak),
            ..Default::default()
        });
        for beat in 0..64 {
            store.maybe_advance_on_beat(beat);
        }
        assert_eq!(store.queue_index, 0);
    }

    #[test]
    fn volume_peak_trigger_does_nothing_in_beat_mode() {
        let mut store = store_with_two_queued();
        store.queue_enabled = true;
        store.maybe_advance_on_volume_peak(1.0, 0.0);
        assert_eq!(store.queue_index, 0);
    }

    /// Settles `detect_volume_peak`'s rolling average at a steady 0.3
    /// and returns the queue index once it is there. The average
    /// starts at 0, so the first non-silent samples of ANY signal read
    /// as a transient (identical in the TS source); every assertion
    /// below is made relative to where that leaves the index, not
    /// against a hardcoded 0.
    fn settle_at_steady_level(store: &mut OverlayStore) -> usize {
        store.queue_enabled = true;
        store.update_overlay_queue_trigger(BeatTriggerConfigPatch {
            mode: Some(BeatTriggerMode::VolumePeak),
            ..Default::default()
        });
        for i in 0..400 {
            store.maybe_advance_on_volume_peak(0.3, i as f64);
        }
        store.queue_index
    }

    #[test]
    fn a_steady_level_does_not_keep_advancing_the_queue() {
        let mut store = store_with_two_queued();
        let settled = settle_at_steady_level(&mut store);
        // Well past the 500 ms cooldown, so a trigger here would be a
        // real threshold crossing, not a suppressed one.
        store.maybe_advance_on_volume_peak(0.3, 5_000.0);
        assert_eq!(store.queue_index, settled);
    }

    #[test]
    fn volume_peak_trigger_advances_on_a_loud_transient() {
        let mut store = store_with_two_queued();
        let settled = settle_at_steady_level(&mut store);
        store.maybe_advance_on_volume_peak(3.0, 5_000.0);
        assert_ne!(store.queue_index, settled);
    }

    #[test]
    fn volume_peak_trigger_respects_its_own_cooldown() {
        let mut store = store_with_two_queued();
        settle_at_steady_level(&mut store);
        store.maybe_advance_on_volume_peak(3.0, 5_000.0);
        let after_peak = store.queue_index;
        // 100 ms later, still inside the 500 ms cooldown
        store.maybe_advance_on_volume_peak(3.0, 5_100.0);
        assert_eq!(store.queue_index, after_peak);
    }
}

mod overlay_transform_tests {
    use super::*;

    #[test]
    fn a_static_overlay_transform_is_its_own_fields() {
        let ov = Overlay { x: 0.25, y: 0.75, rotation: 30.0, scale: 2.0, ..Default::default() };
        let t = overlay_transform_at(&ov, 12.34, false);
        assert_eq!(t, OverlayTransform { x: 0.25, y: 0.75, rotation_deg: 30.0, scale: 2.0 });
    }

    #[test]
    fn spin_adds_degrees_per_second_on_top_of_rotation() {
        let ov = Overlay { rotation: 10.0, spin: 90.0, ..Default::default() };
        assert_eq!(overlay_transform_at(&ov, 2.0, false).rotation_deg, 190.0);
        assert_eq!(overlay_transform_at(&ov, -1.0, false).rotation_deg, -80.0);
    }

    #[test]
    fn beat_pulse_only_scales_a_beat_reactive_overlay() {
        let inert = Overlay { scale: 1.0, beat_scale: 1.25, ..Default::default() };
        assert_eq!(overlay_transform_at(&inert, 0.0, true).scale, 1.0);
        let reactive = Overlay { scale: 1.0, beat_scale: 1.25, beat_reactive: true, ..Default::default() };
        assert_eq!(overlay_transform_at(&reactive, 0.0, true).scale, 1.25);
        assert_eq!(overlay_transform_at(&reactive, 0.0, false).scale, 1.0);
    }

    #[test]
    fn zero_drift_never_moves_the_overlay() {
        let ov = Overlay { x: 0.5, y: 0.5, ..Default::default() };
        for step in 0..40 {
            let t = overlay_transform_at(&ov, step as f64 * 0.25, false);
            assert_eq!((t.x, t.y), (0.5, 0.5));
        }
    }

    #[test]
    fn drift_starts_at_the_anchor_and_peaks_at_a_full_amplitude() {
        // speed = 1 => one 0->1 leg per second, full cycle 2 s.
        let ov = Overlay { x: 0.5, y: 0.5, drift_x: 1.0, drift_y: -1.0, ..Default::default() };
        let at0 = overlay_transform_at(&ov, 0.0, false);
        assert!((at0.x - 0.5).abs() < 1e-9);
        assert!((at0.y - 0.5).abs() < 1e-9);
        let at1 = overlay_transform_at(&ov, 1.0, false);
        assert!((at1.x - (0.5 + DRIFT_AMPLITUDE_X)).abs() < 1e-9);
        assert!((at1.y - (0.5 - DRIFT_AMPLITUDE_Y)).abs() < 1e-9);
        // `alternate` brings it back to the anchor after a full cycle.
        let at2 = overlay_transform_at(&ov, 2.0, false);
        assert!((at2.x - 0.5).abs() < 1e-9);
    }

    #[test]
    fn drift_stays_within_its_amplitude_envelope_forever() {
        let ov = Overlay { x: 0.5, y: 0.5, drift_x: 0.4, drift_y: 0.9, ..Default::default() };
        for step in 0..500 {
            let t = overlay_transform_at(&ov, step as f64 * 0.137, false);
            assert!((t.x - 0.5).abs() <= 0.4 * DRIFT_AMPLITUDE_X + 1e-9);
            assert!((t.y - 0.5).abs() <= 0.9 * DRIFT_AMPLITUDE_Y + 1e-9);
        }
    }

    #[test]
    fn a_drift_below_the_speed_floor_still_oscillates_at_the_floor_rate() {
        // `Math.max(..., 0.05)` in the source: a 0.01 drift still moves,
        // just at the 20 s-per-leg floor rate rather than 100 s.
        let ov = Overlay { x: 0.5, y: 0.5, drift_x: 0.01, ..Default::default() };
        let peak = overlay_transform_at(&ov, 20.0, false);
        assert!((peak.x - (0.5 + 0.01 * DRIFT_AMPLITUDE_X)).abs() < 1e-9);
    }
}

mod parse_hex_color_tests {
    use super::*;

    #[test]
    fn parses_the_six_digit_form_with_and_without_a_hash() {
        assert_eq!(parse_hex_color("#ff2d78"), [0xff, 0x2d, 0x78]);
        assert_eq!(parse_hex_color("00FF80"), [0x00, 0xff, 0x80]);
    }

    #[test]
    fn expands_the_three_digit_shorthand() {
        assert_eq!(parse_hex_color("#f0a"), [0xff, 0x00, 0xaa]);
    }

    #[test]
    fn falls_back_to_white_on_anything_unparseable() {
        assert_eq!(parse_hex_color(""), [255, 255, 255]);
        assert_eq!(parse_hex_color("#gg0011"), [255, 255, 255]);
        assert_eq!(parse_hex_color("rebeccapurple"), [255, 255, 255]);
    }

    #[test]
    fn parses_the_overlay_default_color() {
        assert_eq!(parse_hex_color(&Overlay::default().color), [255, 255, 255]);
    }
}

/// Port of `overlay-queue.test.ts:81-108`'s `visibleOverlayIds` tests.
mod visible_overlay_ids_tests {
    use super::*;

    fn overlay(id: &str, in_queue: bool) -> Overlay {
        Overlay { id: id.to_string(), in_queue, ..Default::default() }
    }

    #[test]
    fn zero_checked_all_unchecked_ones_are_visible() {
        let overlays = vec![overlay("a", false), overlay("b", false)];
        let ids = visible_overlay_ids(&overlays, 0);
        assert_eq!(ids, HashSet::from(["a", "b"]));
    }

    #[test]
    fn one_checked_always_visible_plus_the_unchecked_ones() {
        let overlays = vec![overlay("a", true), overlay("b", false)];
        let ids = visible_overlay_ids(&overlays, 0);
        assert_eq!(ids, HashSet::from(["a", "b"]));
    }

    #[test]
    fn multiple_checked_only_the_one_at_the_active_index_plus_the_unchecked_ones() {
        let overlays = vec![overlay("a", true), overlay("b", true), overlay("c", false)];
        let ids = visible_overlay_ids(&overlays, 1);
        assert_eq!(ids, HashSet::from(["b", "c"]));
    }

    #[test]
    fn out_of_bounds_index_clean_fallback_no_crash_first_checked_one_visible() {
        let overlays = vec![overlay("a", true), overlay("b", true)];
        let ids = visible_overlay_ids(&overlays, 99);
        assert_eq!(ids, HashSet::from(["a"]));
    }
}
