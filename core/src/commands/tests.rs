use super::*;

#[derive(Default)]
struct MockCtx {
    crossfader: f64,
    active_deck: Deck,
    navigate_preset_calls: Vec<(Deck, i32)>,
    toggle_playlist_calls: Vec<Deck>,
    playlist_next_calls: Vec<Deck>,
    playlist_prev_calls: Vec<Deck>,
    advance_overlay_queue_calls: Vec<i32>,
    color_hue_a: f64,
    color_sat_a: f64,
    color_bright_a: f64,
    color_contrast_a: f64,
    color_invert_a: f64,
    color_hue_b: f64,
    color_sat_b: f64,
    color_bright_b: f64,
    color_contrast_b: f64,
    color_invert_b: f64,
    composite_blend_calls: Vec<(usize, f64)>,
    composite_luma_black_calls: Vec<(usize, f64)>,
    composite_luma_white_calls: Vec<(usize, f64)>,
    composite_color_hue_calls: Vec<(usize, f64)>,
    composite_color_tol_calls: Vec<(usize, f64)>,
    recall_snapshot_calls: Vec<usize>,
    toggle_timeline_calls: u32,
    toggle_strobe_calls: u32,
    /// `(multiplier name, slot, value)` for all 8 Time setters: one
    /// log instead of 8 near-identical `Vec`s, so a test can assert
    /// which multiplier a `CommandId::Time*` actually reached.
    time_calls: Vec<(&'static str, usize, f64)>,
    /// `(slot, n, value)` for `set_q_var`, so a test can assert which of
    /// the 128 `CommandId::Qvar*` reached which (deck, q-var) pair.
    q_var_calls: Vec<(usize, usize, f64)>,
}

impl CommandContext for MockCtx {
    fn get_crossfader(&self) -> f64 {
        self.crossfader
    }
    fn set_crossfader(&mut self, v: f64) {
        self.crossfader = v;
    }
    fn get_active_deck(&self) -> Deck {
        self.active_deck
    }
    fn switch_active_deck(&mut self) {
        self.active_deck = match self.active_deck {
            Deck::A => Deck::B,
            Deck::B => Deck::A,
        };
    }
    fn navigate_preset(&mut self, deck: Deck, direction: i32) {
        self.navigate_preset_calls.push((deck, direction));
    }
    fn toggle_playlist(&mut self, deck: Deck) {
        self.toggle_playlist_calls.push(deck);
    }
    fn playlist_next(&mut self, deck: Deck) {
        self.playlist_next_calls.push(deck);
    }
    fn playlist_prev(&mut self, deck: Deck) {
        self.playlist_prev_calls.push(deck);
    }
    fn get_playlist_playing(&self, _deck: Deck) -> bool {
        false
    }
    fn advance_overlay_queue(&mut self, direction: i32) {
        self.advance_overlay_queue_calls.push(direction);
    }
    fn set_color_hue_a(&mut self, v: f64) {
        self.color_hue_a = v;
    }
    fn set_color_sat_a(&mut self, v: f64) {
        self.color_sat_a = v;
    }
    fn set_color_bright_a(&mut self, v: f64) {
        self.color_bright_a = v;
    }
    fn set_color_contrast_a(&mut self, v: f64) {
        self.color_contrast_a = v;
    }
    fn set_color_invert_a(&mut self, v: f64) {
        self.color_invert_a = v;
    }
    fn set_color_hue_b(&mut self, v: f64) {
        self.color_hue_b = v;
    }
    fn set_color_sat_b(&mut self, v: f64) {
        self.color_sat_b = v;
    }
    fn set_color_bright_b(&mut self, v: f64) {
        self.color_bright_b = v;
    }
    fn set_color_contrast_b(&mut self, v: f64) {
        self.color_contrast_b = v;
    }
    fn set_color_invert_b(&mut self, v: f64) {
        self.color_invert_b = v;
    }
    fn set_composite_blend(&mut self, slot: usize, v: f64) {
        self.composite_blend_calls.push((slot, v));
    }
    fn set_composite_luma_black(&mut self, slot: usize, v: f64) {
        self.composite_luma_black_calls.push((slot, v));
    }
    fn set_composite_luma_white(&mut self, slot: usize, v: f64) {
        self.composite_luma_white_calls.push((slot, v));
    }
    fn set_composite_color_hue(&mut self, slot: usize, v: f64) {
        self.composite_color_hue_calls.push((slot, v));
    }
    fn set_composite_color_tol(&mut self, slot: usize, v: f64) {
        self.composite_color_tol_calls.push((slot, v));
    }
    fn recall_snapshot(&mut self, slot: usize) {
        self.recall_snapshot_calls.push(slot);
    }
    fn toggle_timeline(&mut self) {
        self.toggle_timeline_calls += 1;
    }
    fn toggle_strobe(&mut self) {
        self.toggle_strobe_calls += 1;
    }
    fn set_time_speed(&mut self, slot: usize, v: f64) {
        self.time_calls.push(("speed", slot, v));
    }
    fn set_time_zoom(&mut self, slot: usize, v: f64) {
        self.time_calls.push(("zoom", slot, v));
    }
    fn set_time_rot(&mut self, slot: usize, v: f64) {
        self.time_calls.push(("rot", slot, v));
    }
    fn set_time_warp(&mut self, slot: usize, v: f64) {
        self.time_calls.push(("warp", slot, v));
    }
    fn set_time_dx(&mut self, slot: usize, v: f64) {
        self.time_calls.push(("dx", slot, v));
    }
    fn set_time_dy(&mut self, slot: usize, v: f64) {
        self.time_calls.push(("dy", slot, v));
    }
    fn set_time_stretch(&mut self, slot: usize, v: f64) {
        self.time_calls.push(("stretch", slot, v));
    }
    fn set_time_wave(&mut self, slot: usize, v: f64) {
        self.time_calls.push(("wave", slot, v));
    }
    fn set_q_var(&mut self, slot: usize, n: usize, v: f64) {
        self.q_var_calls.push((slot, n, v));
    }
}

fn make_ctx() -> MockCtx {
    MockCtx {
        crossfader: 0.5,
        active_deck: Deck::A,
        ..Default::default()
    }
}

mod command_registry {
    use super::*;

    #[test]
    fn register_and_get() {
        let mut reg = CommandRegistry::new();
        reg.register(Command { id: CommandId::Crossfader, label: "X", kind: CommandKind::Range, run: noop });
        assert!(reg.get(CommandId::Crossfader).is_some());
    }

    #[test]
    fn dispatch_calls_run_with_the_correct_value() {
        let mut reg = CommandRegistry::new();
        reg.register(Command { id: CommandId::Crossfader, label: "X", kind: CommandKind::Range, run: |v, ctx| ctx.set_crossfader(v) });
        let mut ctx = make_ctx();
        reg.dispatch(CommandId::Crossfader, 0.75, &mut ctx);
        assert_eq!(ctx.crossfader, 0.75);
    }

    #[test]
    fn dispatch_on_an_unknown_id_does_not_crash() {
        let reg = CommandRegistry::new();
        let mut ctx = make_ctx();
        reg.dispatch(CommandId::StrobeToggle, 1.0, &mut ctx);
    }

    #[test]
    fn dispatch_ignores_non_finite_values() {
        // Regression test: NaN/±∞ arriving from an
        // unauthenticated remote input (OSC UDP, remote-ws) must not
        // reach any `run` closure. NaN in particular survives
        // `f64::clamp` and saturates to `0` in `encode_param`'s
        // `round() as i32`, which the Time/Qvar preset channel decodes
        // as a permanent `-2.0` write. One representative host-side
        // setter (Color) and one preset-channel setter (Qvar).
        let reg = create_default_registry();
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut ctx = make_ctx();
            let before = ctx.color_hue_a;
            reg.dispatch(CommandId::ColorHueA, bad, &mut ctx);
            reg.dispatch(CommandId::Qvar1_1, bad, &mut ctx);
            assert_eq!(ctx.color_hue_a, before, "ColorHueA moved on a {bad} dispatch");
            assert!(ctx.q_var_calls.is_empty(), "Qvar1_1 reached its setter on a {bad} dispatch");
        }

        // ...and a finite value on the very same ids still lands, so the
        // guard is not just disabling the commands outright.
        let mut ctx = make_ctx();
        reg.dispatch(CommandId::ColorHueA, 0.25, &mut ctx);
        reg.dispatch(CommandId::Qvar1_1, 0.25, &mut ctx);
        assert_eq!(ctx.color_hue_a, 0.25);
        assert_eq!(ctx.q_var_calls.len(), 1);
    }

    #[test]
    fn all_returns_all_commands() {
        let mut reg = CommandRegistry::new();
        reg.register(Command { id: CommandId::Crossfader, label: "X", kind: CommandKind::Range, run: noop });
        reg.register(Command { id: CommandId::DeckSwitch, label: "Y", kind: CommandKind::Trigger, run: noop });
        assert_eq!(reg.all().len(), 2);
    }

    #[test]
    fn all_preserves_insertion_order_not_hashmap_order() {
        // Regression test: register a batch of ids that would
        // NOT come back sorted by any single obvious key (id/label/enum
        // discriminant), then repeat register() with a different batch:
        // if `all()` were ever backed by a HashMap again, at least one of
        // these arrangements would very likely trip a hash-order mismatch.
        let mut reg = CommandRegistry::new();
        let ids = [
            CommandId::Qvar12_2,
            CommandId::Crossfader,
            CommandId::TimeWave3,
            CommandId::DeckSwitch,
            CommandId::RecallSnapshot4,
        ];
        for id in ids {
            reg.register(Command { id, label: "X", kind: CommandKind::Trigger, run: noop });
        }
        let order: Vec<CommandId> = reg.all().iter().map(|cmd| cmd.id).collect();
        assert_eq!(order, ids);
    }

    #[test]
    fn register_replaces_an_existing_id_in_place_without_moving_it() {
        let mut reg = CommandRegistry::new();
        reg.register(Command { id: CommandId::Crossfader, label: "first", kind: CommandKind::Range, run: noop });
        reg.register(Command { id: CommandId::DeckSwitch, label: "second", kind: CommandKind::Trigger, run: noop });
        reg.register(Command { id: CommandId::Crossfader, label: "replaced", kind: CommandKind::Range, run: noop });
        let order: Vec<CommandId> = reg.all().iter().map(|cmd| cmd.id).collect();
        assert_eq!(order, vec![CommandId::Crossfader, CommandId::DeckSwitch]);
        assert_eq!(reg.get(CommandId::Crossfader).unwrap().label, "replaced");
    }
}

mod default_registry_order {
    use super::*;

    #[test]
    fn all_matches_the_curated_default_commands_construction_order() {
        // `all()`'s order must match `default_commands()`'s
        // construction order (deck controls -> active-deck shortcuts ->
        // M2/M3 -> compositing -> snapshots -> time params -> overlay ->
        // timeline -> q-vars), not alphabetical, not hash order.
        let reg = create_default_registry();
        let order: Vec<CommandId> = reg.all().iter().map(|cmd| cmd.id).collect();
        assert_eq!(order.first(), Some(&CommandId::Crossfader));
        // CompositeBlend0 (compositing group) must come before
        // TimeSpeed0 (time-params group), a relationship no hash-map
        // iteration order could be relied on to reproduce.
        let composite_pos = order.iter().position(|&id| id == CommandId::CompositeBlend0).unwrap();
        let time_pos = order.iter().position(|&id| id == CommandId::TimeSpeed0).unwrap();
        assert!(composite_pos < time_pos);
        // Qvar1_0 (last group) must come after TimelineToggle (second to last).
        let timeline_pos = order.iter().position(|&id| id == CommandId::TimelineToggle).unwrap();
        let qvar_pos = order.iter().position(|&id| id == CommandId::Qvar1_0).unwrap();
        assert!(timeline_pos < qvar_pos);
        assert_eq!(order.last(), Some(&CommandId::Qvar32_3));
    }
}

mod default_registry_base_commands {
    use super::*;

    #[test]
    fn contains_the_11_legacy_midi_commands() {
        let reg = create_default_registry();
        for id in [
            CommandId::Crossfader, CommandId::PresetPrevA, CommandId::PresetNextA,
            CommandId::PresetPrevB, CommandId::PresetNextB, CommandId::PlaylistToggleA,
            CommandId::PlaylistToggleB, CommandId::PlaylistPrevA, CommandId::PlaylistNextA,
            CommandId::PlaylistPrevB, CommandId::PlaylistNextB,
        ] {
            assert!(reg.get(id).is_some(), "missing: {id:?}");
        }
    }

    #[test]
    fn crossfader_range_calls_set_crossfader_with_the_value() {
        let reg = create_default_registry();
        let mut ctx = make_ctx();
        reg.dispatch(CommandId::Crossfader, 0.3, &mut ctx);
        assert_eq!(ctx.get_crossfader(), 0.3);
    }

    #[test]
    fn color_range_commands_call_the_correct_setter_with_the_value() {
        let reg = create_default_registry();
        let mut ctx = make_ctx();
        reg.dispatch(CommandId::ColorHueA, 0.1, &mut ctx);
        assert_eq!(ctx.color_hue_a, 0.1);
        reg.dispatch(CommandId::ColorSatA, 0.2, &mut ctx);
        assert_eq!(ctx.color_sat_a, 0.2);
        reg.dispatch(CommandId::ColorBrightA, 0.3, &mut ctx);
        assert_eq!(ctx.color_bright_a, 0.3);
        reg.dispatch(CommandId::ColorContrastA, 0.4, &mut ctx);
        assert_eq!(ctx.color_contrast_a, 0.4);
        reg.dispatch(CommandId::ColorInvertA, 0.5, &mut ctx);
        assert_eq!(ctx.color_invert_a, 0.5);
        reg.dispatch(CommandId::ColorHueB, 0.6, &mut ctx);
        assert_eq!(ctx.color_hue_b, 0.6);
        reg.dispatch(CommandId::ColorSatB, 0.7, &mut ctx);
        assert_eq!(ctx.color_sat_b, 0.7);
        reg.dispatch(CommandId::ColorBrightB, 0.8, &mut ctx);
        assert_eq!(ctx.color_bright_b, 0.8);
        reg.dispatch(CommandId::ColorContrastB, 0.9, &mut ctx);
        assert_eq!(ctx.color_contrast_b, 0.9);
        reg.dispatch(CommandId::ColorInvertB, 1.0, &mut ctx);
        assert_eq!(ctx.color_invert_b, 1.0);
    }

    #[test]
    fn preset_next_a_navigates_preset_a_1() {
        let reg = create_default_registry();
        let mut ctx = make_ctx();
        reg.dispatch(CommandId::PresetNextA, 1.0, &mut ctx);
        assert_eq!(ctx.navigate_preset_calls, vec![(Deck::A, 1)]);
    }

    #[test]
    fn preset_prev_b_navigates_preset_b_minus_1() {
        let reg = create_default_registry();
        let mut ctx = make_ctx();
        reg.dispatch(CommandId::PresetPrevB, 1.0, &mut ctx);
        assert_eq!(ctx.navigate_preset_calls, vec![(Deck::B, -1)]);
    }

    #[test]
    fn playlist_toggle_a_toggles_playlist_a() {
        let reg = create_default_registry();
        let mut ctx = make_ctx();
        reg.dispatch(CommandId::PlaylistToggleA, 1.0, &mut ctx);
        assert_eq!(ctx.toggle_playlist_calls, vec![Deck::A]);
    }

    #[test]
    fn playlist_next_b_advances_playlist_b() {
        let reg = create_default_registry();
        let mut ctx = make_ctx();
        reg.dispatch(CommandId::PlaylistNextB, 1.0, &mut ctx);
        assert_eq!(ctx.playlist_next_calls, vec![Deck::B]);
    }
}

mod default_registry_compositing_commands {
    use super::*;

    #[test]
    fn contains_the_20_composite_lumakey_colorkey_commands_for_the_4_slots() {
        let reg = create_default_registry();
        let ids: [CommandId; 20] = [
            CommandId::CompositeBlend0, CommandId::CompositeBlend1, CommandId::CompositeBlend2, CommandId::CompositeBlend3,
            CommandId::LumakeyBlack0, CommandId::LumakeyBlack1, CommandId::LumakeyBlack2, CommandId::LumakeyBlack3,
            CommandId::LumakeyWhite0, CommandId::LumakeyWhite1, CommandId::LumakeyWhite2, CommandId::LumakeyWhite3,
            CommandId::ColorkeyHue0, CommandId::ColorkeyHue1, CommandId::ColorkeyHue2, CommandId::ColorkeyHue3,
            CommandId::ColorkeyTolerance0, CommandId::ColorkeyTolerance1, CommandId::ColorkeyTolerance2, CommandId::ColorkeyTolerance3,
        ];
        for id in ids {
            let cmd = reg.get(id);
            assert!(cmd.is_some(), "missing: {id:?}");
            assert_eq!(cmd.unwrap().kind, CommandKind::Range, "wrong kind: {id:?}");
        }
    }

    #[test]
    fn composite_range_commands_call_the_correct_setter_with_the_slot_and_value() {
        let reg = create_default_registry();
        let mut ctx = make_ctx();
        reg.dispatch(CommandId::CompositeBlend2, 0.75, &mut ctx);
        assert_eq!(ctx.composite_blend_calls, vec![(2, 0.75)]);
        reg.dispatch(CommandId::LumakeyBlack1, 0.3, &mut ctx);
        assert_eq!(ctx.composite_luma_black_calls, vec![(1, 0.3)]);
        reg.dispatch(CommandId::LumakeyWhite3, 0.9, &mut ctx);
        assert_eq!(ctx.composite_luma_white_calls, vec![(3, 0.9)]);
        reg.dispatch(CommandId::ColorkeyHue0, 0.2, &mut ctx);
        assert_eq!(ctx.composite_color_hue_calls, vec![(0, 0.2)]);
        reg.dispatch(CommandId::ColorkeyTolerance3, 0.6, &mut ctx);
        assert_eq!(ctx.composite_color_tol_calls, vec![(3, 0.6)]);
    }
}

mod default_registry_time_params {
    use super::*;

    /// Every `CommandId::Time*` paired with the multiplier and slot it
    /// must reach: the whole 8x4 matrix, since a copy-paste slip in the
    /// registry table is exactly the failure this guards.
    const MATRIX: [(CommandId, &str, usize); 32] = [
        (CommandId::TimeSpeed0, "speed", 0), (CommandId::TimeSpeed1, "speed", 1),
        (CommandId::TimeSpeed2, "speed", 2), (CommandId::TimeSpeed3, "speed", 3),
        (CommandId::TimeZoom0, "zoom", 0), (CommandId::TimeZoom1, "zoom", 1),
        (CommandId::TimeZoom2, "zoom", 2), (CommandId::TimeZoom3, "zoom", 3),
        (CommandId::TimeRot0, "rot", 0), (CommandId::TimeRot1, "rot", 1),
        (CommandId::TimeRot2, "rot", 2), (CommandId::TimeRot3, "rot", 3),
        (CommandId::TimeWarp0, "warp", 0), (CommandId::TimeWarp1, "warp", 1),
        (CommandId::TimeWarp2, "warp", 2), (CommandId::TimeWarp3, "warp", 3),
        (CommandId::TimeDx0, "dx", 0), (CommandId::TimeDx1, "dx", 1),
        (CommandId::TimeDx2, "dx", 2), (CommandId::TimeDx3, "dx", 3),
        (CommandId::TimeDy0, "dy", 0), (CommandId::TimeDy1, "dy", 1),
        (CommandId::TimeDy2, "dy", 2), (CommandId::TimeDy3, "dy", 3),
        (CommandId::TimeStretch0, "stretch", 0), (CommandId::TimeStretch1, "stretch", 1),
        (CommandId::TimeStretch2, "stretch", 2), (CommandId::TimeStretch3, "stretch", 3),
        (CommandId::TimeWave0, "wave", 0), (CommandId::TimeWave1, "wave", 1),
        (CommandId::TimeWave2, "wave", 2), (CommandId::TimeWave3, "wave", 3),
    ];

    #[test]
    fn every_time_command_reaches_its_own_multiplier_and_slot() {
        let reg = create_default_registry();
        for (id, name, slot) in MATRIX {
            let mut ctx = make_ctx();
            reg.dispatch(id, 0.5, &mut ctx);
            assert_eq!(ctx.time_calls, vec![(name, slot, 1.0)], "{id:?}");
        }
    }

    #[test]
    fn all_32_are_registered_as_range_commands() {
        let reg = create_default_registry();
        for (id, ..) in MATRIX {
            let cmd = reg.get(id).unwrap_or_else(|| panic!("missing: {id:?}"));
            assert_eq!(cmd.kind, CommandKind::Range, "{id:?}");
        }
    }

    #[test]
    fn a_dispatched_0_to_1_value_spans_the_panels_0_to_2_range() {
        // Half travel on a MIDI fader must land exactly on neutral, or a
        // mapped controller could never reach "no effect".
        let reg = create_default_registry();
        for (v01, expected) in [(0.0, 0.0), (0.5, 1.0), (1.0, 2.0)] {
            let mut ctx = make_ctx();
            reg.dispatch(CommandId::TimeZoom1, v01, &mut ctx);
            assert_eq!(ctx.time_calls, vec![("zoom", 1, expected)]);
        }
    }

    #[test]
    fn out_of_range_values_are_clamped_to_the_panel_range() {
        let reg = create_default_registry();
        let mut ctx = make_ctx();
        reg.dispatch(CommandId::TimeRot0, 5.0, &mut ctx);
        reg.dispatch(CommandId::TimeRot0, -5.0, &mut ctx);
        assert_eq!(ctx.time_calls, vec![("rot", 0, 2.0), ("rot", 0, 0.0)]);
    }
}

mod default_registry_recall_snapshots {
    use super::*;

    #[test]
    fn contains_the_8_recall_snapshot_0_7_triggers() {
        let reg = create_default_registry();
        let ids: [CommandId; 8] = [
            CommandId::RecallSnapshot0, CommandId::RecallSnapshot1, CommandId::RecallSnapshot2, CommandId::RecallSnapshot3,
            CommandId::RecallSnapshot4, CommandId::RecallSnapshot5, CommandId::RecallSnapshot6, CommandId::RecallSnapshot7,
        ];
        for id in ids {
            let cmd = reg.get(id);
            assert!(cmd.is_some(), "missing: {id:?}");
            assert_eq!(cmd.unwrap().kind, CommandKind::Trigger);
        }
    }

    #[test]
    fn recall_snapshot_triggers_call_the_correct_slot() {
        let reg = create_default_registry();
        let mut ctx = make_ctx();
        reg.dispatch(CommandId::RecallSnapshot0, 1.0, &mut ctx);
        reg.dispatch(CommandId::RecallSnapshot7, 1.0, &mut ctx);
        assert_eq!(ctx.recall_snapshot_calls, vec![0, 7]);
    }
}

mod default_registry_timeline_toggle {
    use super::*;

    #[test]
    fn contains_timeline_toggle_as_a_trigger() {
        let reg = create_default_registry();
        let cmd = reg.get(CommandId::TimelineToggle);
        assert!(cmd.is_some());
        assert_eq!(cmd.unwrap().kind, CommandKind::Trigger);
    }

    #[test]
    fn dispatch_calls_toggle_timeline() {
        let reg = create_default_registry();
        let mut ctx = make_ctx();
        reg.dispatch(CommandId::TimelineToggle, 1.0, &mut ctx);
        assert_eq!(ctx.toggle_timeline_calls, 1);
    }
}

mod default_registry_strobe_toggle {
    use super::*;

    #[test]
    fn contains_strobe_toggle_as_a_trigger() {
        let reg = create_default_registry();
        let cmd = reg.get(CommandId::StrobeToggle);
        assert!(cmd.is_some());
        assert_eq!(cmd.unwrap().kind, CommandKind::Trigger);
    }

    #[test]
    fn dispatch_calls_toggle_strobe() {
        let reg = create_default_registry();
        let mut ctx = make_ctx();
        reg.dispatch(CommandId::StrobeToggle, 1.0, &mut ctx);
        assert_eq!(ctx.toggle_strobe_calls, 1);
    }
}

mod default_registry_active_deck_shortcuts {
    use super::*;

    #[test]
    fn crossfader_left_decrements_by_0_05() {
        let reg = create_default_registry();
        let mut ctx = make_ctx();
        ctx.set_crossfader(0.5);
        reg.dispatch(CommandId::CrossfaderLeft, 1.0, &mut ctx);
        assert!((ctx.get_crossfader() - 0.45).abs() < 1e-9);
    }

    #[test]
    fn crossfader_right_increments_by_0_05() {
        let reg = create_default_registry();
        let mut ctx = make_ctx();
        ctx.set_crossfader(0.5);
        reg.dispatch(CommandId::CrossfaderRight, 1.0, &mut ctx);
        assert!((ctx.get_crossfader() - 0.55).abs() < 1e-9);
    }

    #[test]
    fn crossfader_left_is_clamped_to_0() {
        let reg = create_default_registry();
        let mut ctx = make_ctx();
        ctx.set_crossfader(0.02);
        reg.dispatch(CommandId::CrossfaderLeft, 1.0, &mut ctx);
        assert_eq!(ctx.get_crossfader(), 0.0);
    }

    #[test]
    fn crossfader_right_is_clamped_to_1() {
        let reg = create_default_registry();
        let mut ctx = make_ctx();
        ctx.set_crossfader(0.98);
        reg.dispatch(CommandId::CrossfaderRight, 1.0, &mut ctx);
        assert_eq!(ctx.get_crossfader(), 1.0);
    }

    #[test]
    fn deck_switch_toggles_a_and_b() {
        let reg = create_default_registry();
        let mut ctx = make_ctx();
        assert_eq!(ctx.get_active_deck(), Deck::A);
        reg.dispatch(CommandId::DeckSwitch, 1.0, &mut ctx);
        assert_eq!(ctx.get_active_deck(), Deck::B);
        reg.dispatch(CommandId::DeckSwitch, 1.0, &mut ctx);
        assert_eq!(ctx.get_active_deck(), Deck::A);
    }

    #[test]
    fn preset_next_active_uses_the_active_deck() {
        let reg = create_default_registry();
        let mut ctx = make_ctx();
        ctx.switch_active_deck();
        reg.dispatch(CommandId::PresetNextActive, 1.0, &mut ctx);
        assert_eq!(ctx.navigate_preset_calls, vec![(Deck::B, 1)]);
    }

    #[test]
    fn playlist_toggle_active_uses_the_active_deck() {
        let reg = create_default_registry();
        let mut ctx = make_ctx();
        reg.dispatch(CommandId::PlaylistToggleActive, 1.0, &mut ctx);
        assert_eq!(ctx.toggle_playlist_calls, vec![Deck::A]);
    }

    #[test]
    fn playlist_prev_active_uses_the_active_deck() {
        let reg = create_default_registry();
        let mut ctx = make_ctx();
        reg.dispatch(CommandId::PlaylistPrevActive, 1.0, &mut ctx);
        assert_eq!(ctx.playlist_prev_calls, vec![Deck::A]);
    }
}

mod default_registry_time_param_sliders {
    use super::*;

    #[test]
    fn contains_the_32_time_commands_for_the_4_slots() {
        let reg = create_default_registry();
        let ids: [CommandId; 32] = [
            CommandId::TimeSpeed0, CommandId::TimeSpeed1, CommandId::TimeSpeed2, CommandId::TimeSpeed3,
            CommandId::TimeZoom0, CommandId::TimeZoom1, CommandId::TimeZoom2, CommandId::TimeZoom3,
            CommandId::TimeRot0, CommandId::TimeRot1, CommandId::TimeRot2, CommandId::TimeRot3,
            CommandId::TimeWarp0, CommandId::TimeWarp1, CommandId::TimeWarp2, CommandId::TimeWarp3,
            CommandId::TimeDx0, CommandId::TimeDx1, CommandId::TimeDx2, CommandId::TimeDx3,
            CommandId::TimeDy0, CommandId::TimeDy1, CommandId::TimeDy2, CommandId::TimeDy3,
            CommandId::TimeStretch0, CommandId::TimeStretch1, CommandId::TimeStretch2, CommandId::TimeStretch3,
            CommandId::TimeWave0, CommandId::TimeWave1, CommandId::TimeWave2, CommandId::TimeWave3,
        ];
        for id in ids {
            let cmd = reg.get(id);
            assert!(cmd.is_some(), "missing: {id:?}");
            assert_eq!(cmd.unwrap().kind, CommandKind::Range, "wrong kind: {id:?}");
        }
    }
}

mod overlay_queue_commands {
    use super::*;

    #[test]
    fn overlay_queue_next_advances_by_1() {
        let reg = create_default_registry();
        let mut ctx = make_ctx();
        reg.dispatch(CommandId::OverlayQueueNext, 1.0, &mut ctx);
        assert_eq!(ctx.advance_overlay_queue_calls, vec![1]);
    }

    #[test]
    fn overlay_queue_prev_advances_by_minus_1() {
        let reg = create_default_registry();
        let mut ctx = make_ctx();
        reg.dispatch(CommandId::OverlayQueuePrev, 1.0, &mut ctx);
        assert_eq!(ctx.advance_overlay_queue_calls, vec![-1]);
    }
}

mod default_registry_qvar_live_editing {
    use super::*;

    /// The 128 `CommandId::Qvar*` in declaration order: q-var 1..=32
    /// major, deck slot 0..=3 minor. Position `j` therefore *is* the
    /// (slot, n) pair the command must reach: `n = j / 4 + 1`,
    /// `slot = j % 4`, which is what
    /// `every_qvar_command_reaches_its_own_q_var_and_slot` checks, the
    /// same copy-paste guard `default_registry_time_params::MATRIX` is
    /// for Time.
    const QVAR_IDS: [CommandId; 128] = [
        CommandId::Qvar1_0, CommandId::Qvar1_1, CommandId::Qvar1_2, CommandId::Qvar1_3,
        CommandId::Qvar2_0, CommandId::Qvar2_1, CommandId::Qvar2_2, CommandId::Qvar2_3,
        CommandId::Qvar3_0, CommandId::Qvar3_1, CommandId::Qvar3_2, CommandId::Qvar3_3,
        CommandId::Qvar4_0, CommandId::Qvar4_1, CommandId::Qvar4_2, CommandId::Qvar4_3,
        CommandId::Qvar5_0, CommandId::Qvar5_1, CommandId::Qvar5_2, CommandId::Qvar5_3,
        CommandId::Qvar6_0, CommandId::Qvar6_1, CommandId::Qvar6_2, CommandId::Qvar6_3,
        CommandId::Qvar7_0, CommandId::Qvar7_1, CommandId::Qvar7_2, CommandId::Qvar7_3,
        CommandId::Qvar8_0, CommandId::Qvar8_1, CommandId::Qvar8_2, CommandId::Qvar8_3,
        CommandId::Qvar9_0, CommandId::Qvar9_1, CommandId::Qvar9_2, CommandId::Qvar9_3,
        CommandId::Qvar10_0, CommandId::Qvar10_1, CommandId::Qvar10_2, CommandId::Qvar10_3,
        CommandId::Qvar11_0, CommandId::Qvar11_1, CommandId::Qvar11_2, CommandId::Qvar11_3,
        CommandId::Qvar12_0, CommandId::Qvar12_1, CommandId::Qvar12_2, CommandId::Qvar12_3,
        CommandId::Qvar13_0, CommandId::Qvar13_1, CommandId::Qvar13_2, CommandId::Qvar13_3,
        CommandId::Qvar14_0, CommandId::Qvar14_1, CommandId::Qvar14_2, CommandId::Qvar14_3,
        CommandId::Qvar15_0, CommandId::Qvar15_1, CommandId::Qvar15_2, CommandId::Qvar15_3,
        CommandId::Qvar16_0, CommandId::Qvar16_1, CommandId::Qvar16_2, CommandId::Qvar16_3,
        CommandId::Qvar17_0, CommandId::Qvar17_1, CommandId::Qvar17_2, CommandId::Qvar17_3,
        CommandId::Qvar18_0, CommandId::Qvar18_1, CommandId::Qvar18_2, CommandId::Qvar18_3,
        CommandId::Qvar19_0, CommandId::Qvar19_1, CommandId::Qvar19_2, CommandId::Qvar19_3,
        CommandId::Qvar20_0, CommandId::Qvar20_1, CommandId::Qvar20_2, CommandId::Qvar20_3,
        CommandId::Qvar21_0, CommandId::Qvar21_1, CommandId::Qvar21_2, CommandId::Qvar21_3,
        CommandId::Qvar22_0, CommandId::Qvar22_1, CommandId::Qvar22_2, CommandId::Qvar22_3,
        CommandId::Qvar23_0, CommandId::Qvar23_1, CommandId::Qvar23_2, CommandId::Qvar23_3,
        CommandId::Qvar24_0, CommandId::Qvar24_1, CommandId::Qvar24_2, CommandId::Qvar24_3,
        CommandId::Qvar25_0, CommandId::Qvar25_1, CommandId::Qvar25_2, CommandId::Qvar25_3,
        CommandId::Qvar26_0, CommandId::Qvar26_1, CommandId::Qvar26_2, CommandId::Qvar26_3,
        CommandId::Qvar27_0, CommandId::Qvar27_1, CommandId::Qvar27_2, CommandId::Qvar27_3,
        CommandId::Qvar28_0, CommandId::Qvar28_1, CommandId::Qvar28_2, CommandId::Qvar28_3,
        CommandId::Qvar29_0, CommandId::Qvar29_1, CommandId::Qvar29_2, CommandId::Qvar29_3,
        CommandId::Qvar30_0, CommandId::Qvar30_1, CommandId::Qvar30_2, CommandId::Qvar30_3,
        CommandId::Qvar31_0, CommandId::Qvar31_1, CommandId::Qvar31_2, CommandId::Qvar31_3,
        CommandId::Qvar32_0, CommandId::Qvar32_1, CommandId::Qvar32_2, CommandId::Qvar32_3,
    ];

    #[test]
    fn contains_the_128_qvar_n_slot_commands() {
        let reg = create_default_registry();
        for id in QVAR_IDS {
            let cmd = reg.get(id);
            assert!(cmd.is_some(), "missing: {id:?}");
            assert_eq!(cmd.unwrap().kind, CommandKind::Range, "wrong kind: {id:?}");
        }
    }

    #[test]
    fn every_qvar_command_reaches_its_own_q_var_and_slot() {
        // 128 near-identical registry lines is exactly where a
        // copy-pasted slot or q-var number hides, and a wrong one is
        // silent: the knob still moves something.
        let reg = create_default_registry();
        for (j, id) in QVAR_IDS.into_iter().enumerate() {
            let mut ctx = make_ctx();
            reg.dispatch(id, 0.5, &mut ctx);
            assert_eq!(ctx.q_var_calls, vec![(j % 4, j / 4 + 1, 0.0)], "{id:?}");
        }
    }

    #[test]
    fn a_dispatched_0_to_1_value_spans_the_panels_minus_2_to_2_range() {
        // Half travel on a MIDI fader must land exactly on 0, or a
        // mapped controller could never reach the q-var's own neutral.
        let reg = create_default_registry();
        for (v01, expected) in [(0.0, -2.0), (0.25, -1.0), (0.5, 0.0), (1.0, 2.0)] {
            let mut ctx = make_ctx();
            reg.dispatch(CommandId::Qvar7_2, v01, &mut ctx);
            assert_eq!(ctx.q_var_calls, vec![(2, 7, expected)]);
        }
    }

    #[test]
    fn an_out_of_range_dispatch_value_is_clamped_to_the_sliders_range() {
        // `dispatch`'s contract is 0..1, but OSC and remote-ws are
        // outside inputs: a q-var beyond -2..2 would silently overflow
        // the side channel's own range (`engine::preset_patch`), which
        // clamps too but has no way to report it.
        let reg = create_default_registry();
        let mut ctx = make_ctx();
        reg.dispatch(CommandId::Qvar1_0, 9.0, &mut ctx);
        reg.dispatch(CommandId::Qvar1_0, -9.0, &mut ctx);
        assert_eq!(ctx.q_var_calls, vec![(0, 1, 2.0), (0, 1, -2.0)]);
    }
}
