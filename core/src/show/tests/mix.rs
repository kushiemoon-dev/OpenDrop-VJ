use super::super::*;
use crate::beat_trigger::BeatTriggerConfigPatch;
use crate::blend::BlendMode;
use crate::commands::{create_default_registry, CommandId};
use crate::overlay::Overlay;

mod bus_gain {
    use super::*;

    #[test]
    fn a_is_full_at_zero_and_zero_at_one() {
        assert_eq!(bus_gain(DeckBus::A, 0.0), 1.0);
        assert_eq!(bus_gain(DeckBus::A, 1.0), 0.0);
    }

    #[test]
    fn b_is_zero_at_zero_and_full_at_one() {
        assert_eq!(bus_gain(DeckBus::B, 0.0), 0.0);
        assert_eq!(bus_gain(DeckBus::B, 1.0), 1.0);
    }

    #[test]
    fn off_is_always_zero() {
        assert_eq!(bus_gain(DeckBus::Off, 0.0), 0.0);
        assert_eq!(bus_gain(DeckBus::Off, 0.5), 0.0);
        assert_eq!(bus_gain(DeckBus::Off, 1.0), 0.0);
    }

    #[test]
    fn a_and_b_split_evenly_at_midpoint() {
        assert_eq!(bus_gain(DeckBus::A, 0.5), 0.5);
        assert_eq!(bus_gain(DeckBus::B, 0.5), 0.5);
    }
}

mod defaults {
    use super::*;

    #[test]
    fn crossfader_starts_at_zero() {
        assert_eq!(Show::default().crossfader, 0.0);
    }

    #[test]
    fn deck_bus_is_a_b_off_off() {
        assert_eq!(Show::default().deck_bus, [DeckBus::A, DeckBus::B, DeckBus::Off, DeckBus::Off]);
    }

    #[test]
    fn active_deck_starts_at_a() {
        assert_eq!(Show::default().active_deck, Deck::A);
    }

    #[test]
    fn slot_composites_start_default() {
        assert_eq!(Show::default().slot_composites, [DEFAULT_SLOT_COMPOSITE; 4]);
    }

    #[test]
    fn color_params_start_default() {
        let show = Show::default();
        assert_eq!(show.color_params_a, DEFAULT_COLOR_PARAMS);
        assert_eq!(show.color_params_b, DEFAULT_COLOR_PARAMS);
    }

    #[test]
    fn time_params_start_neutral_on_every_slot() {
        assert_eq!(Show::default().time_params, [DeckTimeParams::default(); 4]);
    }

    #[test]
    fn q_var_params_start_unwatched_on_every_slot() {
        assert_eq!(Show::default().q_var_params, [default_q_var_params(); 4]);
    }

    #[test]
    fn locks_start_false() {
        let show = Show::default();
        assert!(!show.lock_a);
        assert!(!show.lock_b);
    }

    #[test]
    fn strobe_starts_disabled() {
        assert!(!Show::default().strobe.enabled);
    }

    #[test]
    fn deck_video_starts_default_on_every_slot() {
        assert_eq!(Show::default().deck_video, std::array::from_fn(|_| VideoState::default()));
    }

    #[test]
    fn rkbx_deck_mapping_starts_fully_unmapped() {
        assert_eq!(Show::default().rkbx_deck_mapping, [None; 4]);
    }
}

mod set_rkbx_deck_mapping {
    use super::*;

    #[test]
    fn succeeds_for_an_unclaimed_slot() {
        let mut show = Show::default();
        assert_eq!(show.set_rkbx_deck_mapping(0, Some(2)), Ok(()));
        assert_eq!(show.rkbx_deck_mapping[0], Some(2));
    }

    #[test]
    fn refuses_a_slot_already_claimed_by_a_different_dj_deck() {
        let mut show = Show::default();
        show.set_rkbx_deck_mapping(0, Some(2)).unwrap();
        let result = show.set_rkbx_deck_mapping(1, Some(2));
        assert!(result.is_err());
        // Untouched: deck 1 stays unmapped, deck 0 keeps its claim.
        assert_eq!(show.rkbx_deck_mapping[1], None);
        assert_eq!(show.rkbx_deck_mapping[0], Some(2));
    }

    #[test]
    fn remapping_the_same_dj_deck_to_a_new_slot_does_not_conflict_with_itself() {
        let mut show = Show::default();
        show.set_rkbx_deck_mapping(0, Some(2)).unwrap();
        assert_eq!(show.set_rkbx_deck_mapping(0, Some(3)), Ok(()));
        assert_eq!(show.rkbx_deck_mapping[0], Some(3));
    }

    #[test]
    fn unmapping_always_succeeds() {
        let mut show = Show::default();
        show.set_rkbx_deck_mapping(0, Some(2)).unwrap();
        assert_eq!(show.set_rkbx_deck_mapping(0, None), Ok(()));
        assert_eq!(show.rkbx_deck_mapping[0], None);
    }
}

mod slot_opacities {
    use super::*;

    #[test]
    fn default_state_is_full_a_zero_elsewhere() {
        assert_eq!(Show::default().slot_opacities(), [1.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn crossfader_at_one_is_full_b_zero_elsewhere() {
        let show = Show { crossfader: 1.0, ..Default::default() };
        assert_eq!(show.slot_opacities(), [0.0, 1.0, 0.0, 0.0]);
    }

    #[test]
    fn crossfader_at_midpoint_splits_a_and_b_evenly() {
        let show = Show { crossfader: 0.5, ..Default::default() };
        assert_eq!(show.slot_opacities(), [0.5, 0.5, 0.0, 0.0]);
    }

    #[test]
    fn off_slots_never_move() {
        let show = Show { crossfader: 1.0, ..Default::default() };
        assert_eq!(show.slot_opacities()[2], 0.0);
        assert_eq!(show.slot_opacities()[3], 0.0);
    }
}

mod deck_bus_next {
    use super::*;

    #[test]
    fn cycles_a_to_b_to_off_and_wraps_to_a() {
        assert_eq!(DeckBus::A.next(), DeckBus::B);
        assert_eq!(DeckBus::B.next(), DeckBus::Off);
        assert_eq!(DeckBus::Off.next(), DeckBus::A);
    }
}

mod select_slot {
    use super::*;

    #[test]
    fn slot_on_bus_a_selects_active_deck_a() {
        let mut show = Show::default();
        show.select_slot(0); // deck_bus[0] == A
        assert_eq!(show.selected_slot, 0);
        assert_eq!(show.active_deck, Deck::A);
    }

    #[test]
    fn slot_on_bus_b_selects_active_deck_b() {
        let mut show = Show::default();
        show.select_slot(1); // deck_bus[1] == B
        assert_eq!(show.selected_slot, 1);
        assert_eq!(show.active_deck, Deck::B);
    }

    #[test]
    fn slot_on_bus_off_falls_back_to_active_deck_a() {
        let mut show = Show::default();
        show.select_slot(2); // deck_bus[2] == Off
        assert_eq!(show.selected_slot, 2);
        assert_eq!(show.active_deck, Deck::A);
    }
}

mod deck_bus_slot_for {
    use super::*;

    #[test]
    fn returns_first_slot_assigned_to_deck_a() {
        let show = Show::default();
        assert_eq!(show.deck_bus_slot_for(Deck::A), Some(0));
    }

    #[test]
    fn returns_first_slot_assigned_to_deck_b() {
        let show = Show::default();
        assert_eq!(show.deck_bus_slot_for(Deck::B), Some(1));
    }

    #[test]
    fn returns_none_when_no_slot_assigned_to_deck() {
        let show = Show { deck_bus: [DeckBus::Off, DeckBus::Off, DeckBus::Off, DeckBus::Off], ..Default::default() };
        assert_eq!(show.deck_bus_slot_for(Deck::A), None);
        assert_eq!(show.deck_bus_slot_for(Deck::B), None);
    }
}

mod command_context {
    use super::*;

    #[test]
    fn set_crossfader_clamps_above_one() {
        let mut show = Show::default();
        show.set_crossfader(1.5);
        assert_eq!(show.get_crossfader(), 1.0);
    }

    #[test]
    fn set_crossfader_clamps_below_zero() {
        let mut show = Show::default();
        show.set_crossfader(-0.5);
        assert_eq!(show.get_crossfader(), 0.0);
    }

    #[test]
    fn switch_active_deck_toggles_a_and_b() {
        let mut show = Show::default();
        assert_eq!(show.get_active_deck(), Deck::A);
        show.switch_active_deck();
        assert_eq!(show.get_active_deck(), Deck::B);
        show.switch_active_deck();
        assert_eq!(show.get_active_deck(), Deck::A);
    }

    #[test]
    fn switch_active_deck_keeps_selected_slot_in_sync() {
        // A keyboard-driven active-deck switch used to leave
        // `selected_slot` (what the Decks panel highlights) pointing
        // at the previous deck's slot.
        let mut show = Show::default(); // deck_bus: [A, B, Off, Off]
        assert_eq!(show.selected_slot, 0);
        show.switch_active_deck(); // now active_deck == B, which is slot 1
        assert_eq!(show.selected_slot, 1);
        show.switch_active_deck(); // back to A, slot 0
        assert_eq!(show.selected_slot, 0);
    }

    #[test]
    fn switch_active_deck_leaves_selected_slot_alone_when_the_new_active_deck_has_no_slot() {
        let mut show = Show { deck_bus: [DeckBus::A, DeckBus::A, DeckBus::Off, DeckBus::Off], selected_slot: 0, ..Default::default() }; // no slot is on B
        show.switch_active_deck(); // active_deck becomes B, but no slot maps to it
        assert_eq!(show.get_active_deck(), Deck::B);
        assert_eq!(show.selected_slot, 0); // unchanged, not reset to something wrong
    }

    #[test]
    fn set_color_hue_a_clamps_to_0_1() {
        let mut show = Show::default();
        show.set_color_hue_a(1.5);
        assert_eq!(show.color_params_a.hue_rotate, 1.0);
        show.set_color_hue_a(-0.5);
        assert_eq!(show.color_params_a.hue_rotate, 0.0);
    }

    #[test]
    fn set_color_sat_a_clamps_to_0_1() {
        let mut show = Show::default();
        show.set_color_sat_a(1.5);
        assert_eq!(show.color_params_a.saturate, 1.0);
        show.set_color_sat_a(-0.5);
        assert_eq!(show.color_params_a.saturate, 0.0);
    }

    #[test]
    fn set_color_bright_a_clamps_to_0_1() {
        let mut show = Show::default();
        show.set_color_bright_a(1.5);
        assert_eq!(show.color_params_a.brightness, 1.0);
        show.set_color_bright_a(-0.5);
        assert_eq!(show.color_params_a.brightness, 0.0);
    }

    #[test]
    fn set_color_contrast_a_clamps_to_0_1() {
        let mut show = Show::default();
        show.set_color_contrast_a(1.5);
        assert_eq!(show.color_params_a.contrast, 1.0);
        show.set_color_contrast_a(-0.5);
        assert_eq!(show.color_params_a.contrast, 0.0);
    }

    #[test]
    fn set_color_invert_a_clamps_to_0_1() {
        let mut show = Show::default();
        show.set_color_invert_a(1.5);
        assert_eq!(show.color_params_a.invert, 1.0);
        show.set_color_invert_a(-0.5);
        assert_eq!(show.color_params_a.invert, 0.0);
    }

    #[test]
    fn set_color_hue_b_clamps_to_0_1() {
        let mut show = Show::default();
        show.set_color_hue_b(1.5);
        assert_eq!(show.color_params_b.hue_rotate, 1.0);
        show.set_color_hue_b(-0.5);
        assert_eq!(show.color_params_b.hue_rotate, 0.0);
    }

    #[test]
    fn set_color_sat_b_clamps_to_0_1() {
        let mut show = Show::default();
        show.set_color_sat_b(1.5);
        assert_eq!(show.color_params_b.saturate, 1.0);
        show.set_color_sat_b(-0.5);
        assert_eq!(show.color_params_b.saturate, 0.0);
    }

    #[test]
    fn set_color_bright_b_clamps_to_0_1() {
        let mut show = Show::default();
        show.set_color_bright_b(1.5);
        assert_eq!(show.color_params_b.brightness, 1.0);
        show.set_color_bright_b(-0.5);
        assert_eq!(show.color_params_b.brightness, 0.0);
    }

    #[test]
    fn set_color_contrast_b_clamps_to_0_1() {
        let mut show = Show::default();
        show.set_color_contrast_b(1.5);
        assert_eq!(show.color_params_b.contrast, 1.0);
        show.set_color_contrast_b(-0.5);
        assert_eq!(show.color_params_b.contrast, 0.0);
    }

    #[test]
    fn set_color_invert_b_clamps_to_0_1() {
        let mut show = Show::default();
        show.set_color_invert_b(1.5);
        assert_eq!(show.color_params_b.invert, 1.0);
        show.set_color_invert_b(-0.5);
        assert_eq!(show.color_params_b.invert, 0.0);
    }

    #[test]
    fn set_composite_blend_decodes_the_value_into_the_right_slot() {
        let mut show = Show::default();
        show.set_composite_blend(2, 0.75); // bucket 3 -> Multiply, see blend_mode_from_value01
        assert_eq!(show.slot_composites[2].blend, BlendMode::Multiply);
        // Other slots untouched.
        assert_eq!(show.slot_composites[0].blend, BlendMode::Normal);
    }

    #[test]
    fn set_composite_luma_black_clamps_to_0_1_in_the_right_slot() {
        let mut show = Show::default();
        show.set_composite_luma_black(1, 1.5);
        assert_eq!(show.slot_composites[1].luma_black, 1.0);
        show.set_composite_luma_black(1, -0.5);
        assert_eq!(show.slot_composites[1].luma_black, 0.0);
        assert_eq!(show.slot_composites[0].luma_black, DEFAULT_SLOT_COMPOSITE.luma_black);
    }

    #[test]
    fn set_composite_luma_white_clamps_to_0_1_in_the_right_slot() {
        let mut show = Show::default();
        show.set_composite_luma_white(3, 1.5);
        assert_eq!(show.slot_composites[3].luma_white, 1.0);
        show.set_composite_luma_white(3, -0.5);
        assert_eq!(show.slot_composites[3].luma_white, 0.0);
    }

    #[test]
    fn set_composite_color_hue_clamps_to_0_1_in_the_right_slot() {
        let mut show = Show::default();
        show.set_composite_color_hue(0, 1.5);
        assert_eq!(show.slot_composites[0].color_hue, 1.0);
        show.set_composite_color_hue(0, -0.5);
        assert_eq!(show.slot_composites[0].color_hue, 0.0);
    }

    #[test]
    fn set_composite_color_tol_clamps_to_0_1_in_the_right_slot() {
        let mut show = Show::default();
        show.set_composite_color_tol(3, 1.5);
        assert_eq!(show.slot_composites[3].color_tol, 1.0);
        show.set_composite_color_tol(3, -0.5);
        assert_eq!(show.slot_composites[3].color_tol, 0.0);
    }

    #[test]
    fn each_time_setter_writes_its_own_multiplier_in_its_own_slot() {
        // One `Show`, all 8 setters on 4 different slots: proves both
        // that no setter writes a neighbour's field and that no slot
        // bleeds into another.
        let mut show = Show::default();
        show.set_time_speed(0, 0.1);
        show.set_time_zoom(1, 0.2);
        show.set_time_rot(2, 0.3);
        show.set_time_warp(3, 0.4);
        show.set_time_dx(0, 0.5);
        show.set_time_dy(1, 0.6);
        show.set_time_stretch(2, 0.7);
        show.set_time_wave(3, 0.8);
        assert_eq!(
            show.time_params[0],
            DeckTimeParams { speed_mult: 0.1, dx_mult: 0.5, ..DeckTimeParams::default() }
        );
        assert_eq!(
            show.time_params[1],
            DeckTimeParams { zoom_mult: 0.2, dy_mult: 0.6, ..DeckTimeParams::default() }
        );
        assert_eq!(
            show.time_params[2],
            DeckTimeParams { rot_mult: 0.3, stretch_mult: 0.7, ..DeckTimeParams::default() }
        );
        assert_eq!(
            show.time_params[3],
            DeckTimeParams { warp_mult: 0.4, wave_mult: 0.8, ..DeckTimeParams::default() }
        );
    }

    #[test]
    fn time_setters_clamp_to_the_panels_0_to_2_range() {
        let mut show = Show::default();
        show.set_time_zoom(0, 9.0);
        assert_eq!(show.time_params[0].zoom_mult, 2.0);
        show.set_time_zoom(0, -9.0);
        assert_eq!(show.time_params[0].zoom_mult, 0.0);
    }

    #[test]
    fn set_q_var_writes_its_own_q_var_in_its_own_slot() {
        let mut show = Show::default();
        show.set_q_var(2, 7, 1.25);
        show.set_q_var(0, 32, -0.5);
        assert_eq!(show.q_var_params[2].value[6], 1.25);
        assert_eq!(show.q_var_params[0].value[31], -0.5);
        // Nothing else moved: one watch on each of two slots, none
        // anywhere else.
        assert_eq!(show.q_var_params[2].enabled.iter().filter(|&&e| e).count(), 1);
        assert_eq!(show.q_var_params[0].enabled.iter().filter(|&&e| e).count(), 1);
        assert_eq!(show.q_var_params[1], default_q_var_params());
        assert_eq!(show.q_var_params[3], default_q_var_params());
    }

    #[test]
    fn set_q_var_enables_the_watch_it_addresses() {
        // A controller or LFO bound to a q-var has to be able to make it
        // move without the panel being opened first.
        let mut show = Show::default();
        assert!(!show.q_var_params[1].enabled[4]);
        show.set_q_var(1, 5, 0.75);
        assert!(show.q_var_params[1].enabled[4]);
        assert_eq!(show.q_var_params[1].value[4], 0.75);
    }

    #[test]
    fn set_q_var_keeps_the_value_it_was_given_when_it_enables() {
        // `with_q_var_watch` resets the value to 0, so calling it after
        // writing the value would zero every dispatch. Pinned because
        // the bug it guards is invisible: the watch appears, the slider
        // just never leaves 0.
        let mut show = Show::default();
        show.set_q_var(0, 1, -1.5);
        assert_eq!(show.q_var_params[0].value[0], -1.5);
        show.set_q_var(0, 1, 1.5);
        assert_eq!(show.q_var_params[0].value[0], 1.5);
    }

    #[test]
    fn set_q_var_clamps_to_the_sliders_minus_2_to_2_range() {
        let mut show = Show::default();
        show.set_q_var(0, 1, 9.0);
        assert_eq!(show.q_var_params[0].value[0], 2.0);
        show.set_q_var(0, 1, -9.0);
        assert_eq!(show.q_var_params[0].value[0], -2.0);
    }

    #[test]
    fn set_q_var_is_a_no_op_for_an_out_of_range_slot_or_q_var() {
        // Reaches `Show` from an OSC/remote-ws payload, so a panic here
        // would be a remotely triggerable crash rather than a bug.
        let mut show = Show::default();
        show.set_q_var(4, 1, 1.0);
        show.set_q_var(0, 0, 1.0);
        show.set_q_var(0, 33, 1.0);
        assert_eq!(show.q_var_params, [default_q_var_params(); 4]);
    }

    #[test]
    fn toggle_strobe_flips_enabled() {
        let mut show = Show::default();
        assert!(!show.strobe.enabled);
        show.toggle_strobe();
        assert!(show.strobe.enabled);
        show.toggle_strobe();
        assert!(!show.strobe.enabled);
    }

    #[test]
    fn advance_overlay_queue_moves_the_stores_queue_index() {
        // This setter used to be an empty no-op.
        let mut show = Show::default();
        show.overlay_store.overlays = vec![
            Overlay { id: "a".to_string(), in_queue: true, ..Default::default() },
            Overlay { id: "b".to_string(), in_queue: true, ..Default::default() },
            Overlay { id: "c".to_string(), in_queue: true, ..Default::default() },
        ];
        assert_eq!(show.overlay_store.queue_index, 0);
        show.advance_overlay_queue(1);
        assert_eq!(show.overlay_store.queue_index, 1);
        show.advance_overlay_queue(-1);
        assert_eq!(show.overlay_store.queue_index, 0);
    }

    #[test]
    fn overlay_queue_next_prev_move_the_index_through_the_registry() {
        // Recipe B parity: the panel's ◀/▶ buttons, a keyboard binding,
        // a MIDI note and an OSC message all reach the store through
        // this one path.
        let registry = create_default_registry();
        let mut show = Show::default();
        show.overlay_store.overlays = vec![
            Overlay { id: "a".to_string(), in_queue: true, ..Default::default() },
            Overlay { id: "b".to_string(), in_queue: true, ..Default::default() },
        ];
        registry.dispatch(CommandId::OverlayQueueNext, 1.0, &mut show);
        assert_eq!(show.overlay_store.queue_index, 1);
        registry.dispatch(CommandId::OverlayQueuePrev, 1.0, &mut show);
        assert_eq!(show.overlay_store.queue_index, 0);
    }
}

/// The two auto-advance signals `Show` forwards into `OverlayStore`:
/// the store's own trigger logic is tested in `core::overlay`; these
/// assert the wiring from `Show`'s side.
mod overlay_queue_wiring {
    use super::*;

    fn show_with_two_queued_overlays() -> Show {
        let mut show = Show::default();
        show.overlay_store.overlays = vec![
            Overlay { id: "a".to_string(), in_queue: true, ..Default::default() },
            Overlay { id: "b".to_string(), in_queue: true, ..Default::default() },
        ];
        show.overlay_store.queue_enabled = true;
        show
    }

    #[test]
    fn on_beat_advances_the_queue_when_the_beat_trigger_matches() {
        let mut show = show_with_two_queued_overlays();
        // `Clock::beat_count` starts at 0, and the default trigger is
        // every 8 beats at offset 0, so the very first beat matches.
        show.on_beat();
        assert_eq!(show.overlay_store.queue_index, 1);
    }

    #[test]
    fn on_beat_leaves_a_disabled_queue_alone() {
        let mut show = show_with_two_queued_overlays();
        show.overlay_store.queue_enabled = false;
        for _ in 0..32 {
            show.on_beat();
        }
        assert_eq!(show.overlay_store.queue_index, 0);
    }

    #[test]
    fn check_volume_peak_triggers_advances_the_queue_on_a_transient() {
        let mut show = show_with_two_queued_overlays();
        show.overlay_store.update_overlay_queue_trigger(BeatTriggerConfigPatch {
            mode: Some(BeatTriggerMode::VolumePeak),
            ..Default::default()
        });
        // Settle the rolling average at a steady level first: it
        // starts at 0, so the first non-silent samples read as a
        // transient (see `core::overlay`'s own tests).
        for i in 0..400 {
            show.check_volume_peak_triggers(0.3, i as f64);
        }
        let settled = show.overlay_store.queue_index;
        show.check_volume_peak_triggers(3.0, 5_000.0);
        assert_ne!(show.overlay_store.queue_index, settled);
    }
}

mod get_command_value {
    use super::*;

    #[test]
    fn reads_back_a_color_param() {
        let mut show = Show::default();
        show.set_color_hue_a(0.3);
        assert_eq!(show.get_command_value(CommandId::ColorHueA), Some(0.3));
    }

    #[test]
    fn reads_back_a_composite_blend_as_its_bucket_center() {
        let show = Show::default(); // slot_composites[0].blend defaults to Normal
        assert_eq!(show.get_command_value(CommandId::CompositeBlend0), Some(blend_mode_to_value01(BlendMode::Normal)));
    }

    #[test]
    fn reads_back_a_composite_key_field_in_the_right_slot() {
        let mut show = Show::default();
        show.set_composite_luma_black(2, 0.4);
        assert_eq!(show.get_command_value(CommandId::LumakeyBlack2), Some(0.4));
        assert_eq!(show.get_command_value(CommandId::LumakeyBlack0), Some(DEFAULT_SLOT_COMPOSITE.luma_black));
    }

    #[test]
    fn returns_none_for_a_command_id_with_no_real_setter() {
        let show = Show::default();
        assert_eq!(show.get_command_value(CommandId::Crossfader), None);
    }

    #[test]
    fn returns_none_for_time_params_even_though_they_have_real_setters() {
        // Deliberate exclusion, not a gap: see `SNAPSHOT_CAPTURABLE_IDS`
        // for the bandwidth and scale-inversion reasons. Pinned as its own
        // test so a future reader sees the intent rather than reading this
        // as an id someone forgot to wire up.
        let mut show = Show::default();
        show.set_time_zoom(0, 1.5);
        assert_eq!(show.time_params[0].zoom_mult, 1.5);
        assert_eq!(show.get_command_value(CommandId::TimeZoom0), None);
        assert!(!show.capture_snapshot_values().keys().any(|id| matches!(
            id,
            CommandId::TimeSpeed0 | CommandId::TimeZoom0 | CommandId::TimeRot0 | CommandId::TimeWave3
        )));
    }

    #[test]
    fn returns_none_for_q_vars_even_though_they_have_real_setters() {
        // Same deliberate exclusion as Time, plus one reason of its own:
        // dispatching a q-var enables its watch, and enabling a watch
        // reloads the deck's preset: see `SNAPSHOT_CAPTURABLE_IDS`.
        let mut show = Show::default();
        show.set_q_var(0, 1, 1.5);
        assert_eq!(show.q_var_params[0].value[0], 1.5);
        assert_eq!(show.get_command_value(CommandId::Qvar1_0), None);
        assert!(!show
            .capture_snapshot_values()
            .keys()
            .any(|id| matches!(id, CommandId::Qvar1_0 | CommandId::Qvar7_2 | CommandId::Qvar32_3)));
    }
}


mod slot_locked {
    use super::*;

    #[test]
    fn a_slot_follows_the_lock_of_the_bus_it_feeds() {
        let mut show = Show { deck_bus: [DeckBus::A, DeckBus::B, DeckBus::Off, DeckBus::A], lock_a: true, ..Show::default() };
        assert!(show.slot_locked(0));
        assert!(!show.slot_locked(1));
        assert!(show.slot_locked(3));
        show.lock_b = true;
        assert!(show.slot_locked(1));
        assert!(!show.slot_locked(2), "an unassigned slot has no lock");
        assert!(!show.slot_locked(99), "an out of range slot is not locked");
    }
}
