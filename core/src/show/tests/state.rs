use super::super::*;
use crate::commands::{create_default_registry, CommandId};
use crate::snapshot::smoothstep;

mod capture_snapshot_values {
    use super::*;

    #[test]
    fn captures_all_30_addressable_command_ids() {
        let show = Show::default();
        assert_eq!(show.capture_snapshot_values().len(), 30);
    }

    #[test]
    fn captures_the_live_value_not_the_default() {
        let mut show = Show::default();
        show.set_color_sat_b(0.9);
        let captured = show.capture_snapshot_values();
        assert_eq!(captured.get(&CommandId::ColorSatB), Some(&0.9));
    }
}

mod recall_snapshot {
    use super::*;

    #[test]
    fn recalling_an_empty_slot_does_nothing() {
        let mut show = Show::default();
        show.recall_snapshot(0);
        assert!(show.active_recall.is_none());
    }

    #[test]
    fn recalling_a_populated_slot_captures_the_current_value_as_start_and_arms_active_recall() {
        let mut show = Show::default();
        show.set_color_hue_a(0.3); // current live value: recall must start from here, not from the target
        show.snapshot_slots[5] = Some(Snapshot { name: "Slot 6".to_string(), values: HashMap::from([(CommandId::ColorHueA, 0.9)]) });

        show.recall_snapshot(5);

        let active = show.active_recall.as_ref().expect("recall should be armed");
        assert_eq!(active.slot, 5);
        assert_eq!(active.start_values, HashMap::from([(CommandId::ColorHueA, 0.3)]));
        assert_eq!(active.elapsed_sec, 0.0);
    }

    #[test]
    fn keyboard_dispatch_through_the_registry_arms_the_correct_slot() {
        let reg = create_default_registry();
        let mut show = Show::default();
        show.snapshot_slots[3] = Some(Snapshot { name: "Slot 4".to_string(), values: HashMap::new() });
        reg.dispatch(CommandId::RecallSnapshot3, 1.0, &mut show);
        assert_eq!(show.active_recall.as_ref().map(|a| a.slot), Some(3));
    }
}

mod tick_recall {
    use super::*;

    #[test]
    fn no_active_recall_returns_no_values() {
        let mut show = Show::default();
        assert!(show.tick_recall(0.5).is_empty());
    }

    #[test]
    fn mid_recall_returns_the_eased_value_and_keeps_the_recall_active() {
        let mut show = Show { snapshot_recall_duration_sec: 1.0, ..Default::default() };
        show.set_color_hue_a(0.0);
        show.snapshot_slots[0] = Some(Snapshot { name: "S".to_string(), values: HashMap::from([(CommandId::ColorHueA, 1.0)]) });
        show.recall_snapshot(0);

        let values = show.tick_recall(0.5);
        assert_eq!(values, vec![(CommandId::ColorHueA, smoothstep(0.5))]);
        assert!(show.active_recall.is_some());
    }

    #[test]
    fn reaching_the_configured_duration_returns_the_exact_target_and_clears_the_recall() {
        let mut show = Show { snapshot_recall_duration_sec: 1.0, ..Default::default() };
        show.snapshot_slots[0] = Some(Snapshot { name: "S".to_string(), values: HashMap::from([(CommandId::ColorHueA, 1.0)]) });
        show.recall_snapshot(0);

        let values = show.tick_recall(1.5); // overshoots the 1s duration
        assert_eq!(values, vec![(CommandId::ColorHueA, 1.0)]);
        assert!(show.active_recall.is_none());
    }

    #[test]
    fn clearing_the_target_slot_mid_recall_cancels_it() {
        let mut show = Show { snapshot_recall_duration_sec: 10.0, ..Default::default() };
        show.snapshot_slots[0] = Some(Snapshot { name: "S".to_string(), values: HashMap::from([(CommandId::ColorHueA, 1.0)]) });
        show.recall_snapshot(0);
        show.snapshot_slots[0] = None; // cleared mid-recall

        let values = show.tick_recall(0.1);
        assert!(values.is_empty());
        assert!(show.active_recall.is_none());
    }
}

mod toggle_timeline {
    use super::*;

    #[test]
    fn toggles_playing_on_and_off() {
        let mut show = Show::default();
        assert!(!show.timeline_playing);
        show.toggle_timeline();
        assert!(show.timeline_playing);
        show.toggle_timeline();
        assert!(!show.timeline_playing);
    }

    #[test]
    fn starting_playback_resets_elapsed_progress_to_the_beginning_of_the_cycle() {
        let mut show = Show::default();
        show.snapshot_slots[0] = Some(Snapshot { name: "A".to_string(), values: HashMap::from([(CommandId::ColorHueA, 0.0)]) });
        show.snapshot_slots[1] = Some(Snapshot { name: "B".to_string(), values: HashMap::from([(CommandId::ColorHueA, 1.0)]) });
        show.timeline_keyframes =
            vec![TimelineKeyframe { slot: 0, time_sec: 0.0 }, TimelineKeyframe { slot: 1, time_sec: 10.0 }];

        show.toggle_timeline(); // start playing
        let mid = show.tick_timeline(5.0); // halfway through the loop
        assert!((mid.iter().find(|(id, _)| *id == CommandId::ColorHueA).unwrap().1 - 0.5).abs() < 1e-9);

        show.toggle_timeline(); // pause
        show.toggle_timeline(); // resume: must restart at the beginning, not jump back to 5.0
        let restarted = show.tick_timeline(0.0);
        assert_eq!(restarted, vec![(CommandId::ColorHueA, 0.0)]);
    }

    #[test]
    fn keyboard_dispatch_through_the_registry_toggles_playing() {
        let reg = create_default_registry();
        let mut show = Show::default();
        reg.dispatch(CommandId::TimelineToggle, 1.0, &mut show);
        assert!(show.timeline_playing);
    }
}

mod tick_timeline {
    use super::*;

    #[test]
    fn not_playing_returns_no_values() {
        let keyframes = vec![TimelineKeyframe { slot: 0, time_sec: 0.0 }, TimelineKeyframe { slot: 1, time_sec: 10.0 }];
        let mut show = Show { timeline_keyframes: keyframes, ..Default::default() };
        assert!(show.tick_timeline(1.0).is_empty());
    }

    #[test]
    fn fewer_than_2_keyframes_returns_no_values_even_while_playing() {
        let keyframes = vec![TimelineKeyframe { slot: 0, time_sec: 0.0 }];
        let mut show = Show { timeline_keyframes: keyframes, ..Default::default() };
        show.toggle_timeline();
        assert!(show.tick_timeline(1.0).is_empty());
    }

    #[test]
    fn accumulates_dt_across_multiple_ticks() {
        let mut show = Show::default();
        show.snapshot_slots[0] = Some(Snapshot { name: "A".to_string(), values: HashMap::from([(CommandId::ColorHueA, 0.0)]) });
        show.snapshot_slots[1] = Some(Snapshot { name: "B".to_string(), values: HashMap::from([(CommandId::ColorHueA, 1.0)]) });
        show.timeline_keyframes =
            vec![TimelineKeyframe { slot: 0, time_sec: 0.0 }, TimelineKeyframe { slot: 1, time_sec: 10.0 }];

        show.toggle_timeline();
        show.tick_timeline(3.0);
        let values = show.tick_timeline(2.0); // total elapsed 5.0 -> midpoint
        assert!((values.iter().find(|(id, _)| *id == CommandId::ColorHueA).unwrap().1 - 0.5).abs() < 1e-9);
    }

    #[test]
    fn loops_back_to_the_start_after_the_full_duration() {
        let mut show = Show::default();
        show.snapshot_slots[0] = Some(Snapshot { name: "A".to_string(), values: HashMap::from([(CommandId::ColorHueA, 0.0)]) });
        show.snapshot_slots[1] = Some(Snapshot { name: "B".to_string(), values: HashMap::from([(CommandId::ColorHueA, 1.0)]) });
        show.timeline_keyframes =
            vec![TimelineKeyframe { slot: 0, time_sec: 0.0 }, TimelineKeyframe { slot: 1, time_sec: 10.0 }];

        show.toggle_timeline();
        let values = show.tick_timeline(12.0); // wraps past the 10s loop, t = 2.0
        let expected = timeline_values_at(&show.timeline_keyframes, &show.snapshot_slots, 2.0);
        assert_eq!(values.into_iter().collect::<HashMap<_, _>>(), expected);
    }
}

mod keyboard_dispatch_through_the_registry {
    use super::*;

    #[test]
    fn crossfader_right_moves_by_0_05_steps() {
        let reg = create_default_registry();
        let mut show = Show::default();
        reg.dispatch(CommandId::CrossfaderRight, 1.0, &mut show);
        assert_eq!(show.crossfader, 0.05);
        reg.dispatch(CommandId::CrossfaderRight, 1.0, &mut show);
        assert_eq!(show.crossfader, 0.10);
    }

    #[test]
    fn crossfader_left_moves_by_0_05_steps_and_clamps_at_zero() {
        let reg = create_default_registry();
        let mut show = Show::default();
        reg.dispatch(CommandId::CrossfaderLeft, 1.0, &mut show);
        assert_eq!(show.crossfader, 0.0); // already at the floor
        show.crossfader = 0.03;
        reg.dispatch(CommandId::CrossfaderLeft, 1.0, &mut show);
        assert_eq!(show.crossfader, 0.0);
    }

    #[test]
    fn deck_switch_toggles_active_deck() {
        let reg = create_default_registry();
        let mut show = Show::default();
        reg.dispatch(CommandId::DeckSwitch, 1.0, &mut show);
        assert_eq!(show.active_deck, Deck::B);
    }
}

mod navigate_preset {
    use super::*;

    fn catalog(names: &[&str]) -> Vec<PresetMeta> {
        names.iter().map(|n| PresetMeta { name: n.to_string(), category: "Other".to_string() }).collect()
    }

    #[test]
    fn forward_advances_the_index_by_one() {
        let mut show = Show { preset_catalog: catalog(&["P0", "P1", "P2"]), ..Default::default() };
        show.navigate_preset(Deck::A, 1);
        assert_eq!(show.preset_index_a, 1);
    }

    #[test]
    fn backward_decrements_the_index_by_one() {
        let mut show = Show { preset_catalog: catalog(&["P0", "P1", "P2"]), preset_index_a: 2, ..Default::default() };
        show.navigate_preset(Deck::A, -1);
        assert_eq!(show.preset_index_a, 1);
    }

    #[test]
    fn forward_wraps_from_the_last_index_to_zero() {
        let mut show = Show { preset_catalog: catalog(&["P0", "P1", "P2"]), preset_index_a: 2, ..Default::default() };
        show.navigate_preset(Deck::A, 1);
        assert_eq!(show.preset_index_a, 0);
    }

    #[test]
    fn backward_wraps_from_zero_to_the_last_index() {
        let mut show = Show { preset_catalog: catalog(&["P0", "P1", "P2"]), preset_index_a: 0, ..Default::default() };
        show.navigate_preset(Deck::A, -1);
        assert_eq!(show.preset_index_a, 2);
    }

    #[test]
    fn deck_a_and_deck_b_have_independent_indices() {
        let mut show = Show { preset_catalog: catalog(&["P0", "P1", "P2"]), ..Default::default() };
        show.navigate_preset(Deck::A, 1);
        assert_eq!(show.preset_index_a, 1);
        assert_eq!(show.preset_index_b, 0);
    }

    #[test]
    fn reports_the_chosen_preset_name_via_the_fired_cell() {
        let mut show = Show { preset_catalog: catalog(&["P0", "P1", "P2"]), ..Default::default() };
        show.navigate_preset(Deck::A, 1);
        assert_eq!(show.fired_preset_a.borrow().as_deref(), Some("P1"));
    }

    #[test]
    fn empty_catalog_is_a_no_op() {
        let mut show = Show::default();
        show.navigate_preset(Deck::A, 1);
        assert_eq!(show.preset_index_a, 0);
        assert!(show.fired_preset_a.borrow().is_none());
    }
}

mod playlist_wiring {
    use super::*;

    #[test]
    fn toggle_playlist_delegates_to_the_playlist_store() {
        let mut show = Show::default();
        show.playlists.add_to_playlist(Deck::A, "p1".to_string());
        show.toggle_playlist(Deck::A);
        assert!(show.playlists.a_playing);
    }

    #[test]
    fn toggle_playlist_twice_starts_then_stops() {
        let mut show = Show::default();
        show.playlists.add_to_playlist(Deck::A, "p1".to_string());
        show.toggle_playlist(Deck::A);
        assert!(show.playlists.a_playing);
        show.toggle_playlist(Deck::A);
        assert!(!show.playlists.a_playing);
    }

    #[test]
    fn get_playlist_playing_reflects_toggle_state() {
        let mut show = Show::default();
        show.playlists.add_to_playlist(Deck::A, "p1".to_string());
        show.toggle_playlist(Deck::A);
        assert!(show.get_playlist_playing(Deck::A));
        assert!(!show.get_playlist_playing(Deck::B));
    }

    #[test]
    fn playlist_next_delegates_to_the_playlist_store() {
        let mut show = Show::default();
        show.playlists.add_to_playlist(Deck::A, "p1".to_string());
        show.playlists.add_to_playlist(Deck::A, "p2".to_string());
        show.playlist_next(Deck::A);
        assert_eq!(show.playlists.engine_a_mut().unwrap().current_index(), 1);
    }

    #[test]
    fn playlist_prev_delegates_to_the_playlist_store() {
        let mut show = Show::default();
        show.playlists.add_to_playlist(Deck::B, "p1".to_string());
        show.playlists.add_to_playlist(Deck::B, "p2".to_string());
        show.playlist_next(Deck::B);
        show.playlist_prev(Deck::B);
        assert_eq!(show.playlists.engine_b_mut().unwrap().current_index(), 0);
    }

    #[test]
    fn playlist_next_on_deck_a_surfaces_the_preset_in_fired_preset_a() {
        let mut show = Show::default();
        show.playlists.add_to_playlist(Deck::A, "p1".to_string());
        show.playlist_next(Deck::A);
        assert_eq!(show.fired_preset_a.borrow().as_deref(), Some("p1"));
    }

    #[test]
    fn playlist_next_on_deck_b_surfaces_the_preset_in_fired_preset_b() {
        let mut show = Show::default();
        show.playlists.add_to_playlist(Deck::B, "p1".to_string());
        show.playlist_next(Deck::B);
        assert_eq!(show.fired_preset_b.borrow().as_deref(), Some("p1"));
    }
}

mod tick_playlists {
    use super::*;

    fn deck_a_with_three_items() -> Show {
        let mut show = Show::default();
        for name in ["p1", "p2", "p3"] {
            show.playlists.add_to_playlist(Deck::A, name.to_string());
        }
        show
    }

    #[test]
    fn advances_a_playing_deck_once_the_interval_has_elapsed() {
        let mut show = deck_a_with_three_items();
        show.playlists.interval_sec = 2.0;
        show.toggle_playlist(Deck::A);
        show.tick_playlists(2000.0);
        assert_eq!(show.playlists.engine_a_mut().unwrap().current_index(), 1);
    }

    #[test]
    fn does_not_advance_before_the_interval_has_elapsed() {
        let mut show = deck_a_with_three_items();
        show.playlists.interval_sec = 2.0;
        show.toggle_playlist(Deck::A);
        show.tick_playlists(1900.0);
        assert_eq!(show.playlists.engine_a_mut().unwrap().current_index(), 0);
    }

    #[test]
    fn does_not_advance_a_deck_that_is_not_playing() {
        let mut show = deck_a_with_three_items();
        show.playlists.interval_sec = 2.0;
        show.tick_playlists(100_000.0);
        assert_eq!(show.playlists.engine_a_mut().unwrap().current_index(), 0);
    }

    #[test]
    fn surfaces_the_advanced_preset_through_take_fired_presets() {
        let mut show = deck_a_with_three_items();
        show.playlists.interval_sec = 2.0;
        show.toggle_playlist(Deck::A);
        let _ = show.take_fired_presets(); // drop the one `start()` fired
        show.tick_playlists(2000.0);
        let out = show.take_fired_presets();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "p2");
    }

    #[test]
    fn does_not_advance_a_beat_synced_deck() {
        let mut show = deck_a_with_three_items();
        show.playlists.interval_sec = 2.0;
        show.toggle_beat_sync(Deck::A);
        show.toggle_playlist(Deck::A);
        show.tick_playlists(100_000.0);
        assert_eq!(show.playlists.engine_a_mut().unwrap().current_index(), 0);
    }

    #[test]
    fn an_interval_change_takes_effect_on_an_already_playing_deck() {
        let mut show = deck_a_with_three_items();
        show.playlists.interval_sec = 60.0;
        show.toggle_playlist(Deck::A);
        show.tick_playlists(3000.0);
        assert_eq!(show.playlists.engine_a_mut().unwrap().current_index(), 0);
        show.playlists.interval_sec = 2.0; // the panel's slider moved mid-play
        show.tick_playlists(16.0);
        assert_eq!(show.playlists.engine_a_mut().unwrap().current_index(), 1);
    }

    #[test]
    fn drives_deck_b_as_well_as_deck_a() {
        let mut show = Show::default();
        show.playlists.add_to_playlist(Deck::B, "b1".to_string());
        show.playlists.add_to_playlist(Deck::B, "b2".to_string());
        show.playlists.interval_sec = 2.0;
        show.toggle_playlist(Deck::B);
        show.tick_playlists(2000.0);
        assert_eq!(show.playlists.engine_b_mut().unwrap().current_index(), 1);
    }
}

mod take_fired_presets {
    use super::*;

    fn catalog(names: &[&str]) -> Vec<PresetMeta> {
        names.iter().map(|n| PresetMeta { name: n.to_string(), category: "Other".to_string() }).collect()
    }

    #[test]
    fn targets_the_slot_assigned_to_deck_a() {
        let mut show = Show { preset_catalog: catalog(&["P0", "P1"]), ..Default::default() }; // deck_bus[0] == A
        show.navigate_preset(Deck::A, 1);
        let out = show.take_fired_presets();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].slot, 0);
        assert_eq!(out[0].name, "P1");
    }

    #[test]
    fn targets_the_slot_assigned_to_deck_b() {
        let mut show = Show { preset_catalog: catalog(&["P0", "P1"]), ..Default::default() }; // deck_bus[1] == B
        show.navigate_preset(Deck::B, 1);
        let out = show.take_fired_presets();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].slot, 1);
        assert_eq!(out[0].name, "P1");
    }

    #[test]
    fn follows_deck_bus_reassignment() {
        let mut show = Show { deck_bus: [DeckBus::B, DeckBus::A, DeckBus::Off, DeckBus::Off], preset_catalog: catalog(&["P0", "P1"]), ..Default::default() };
        show.navigate_preset(Deck::A, 1);
        let out = show.take_fired_presets();
        assert_eq!(out[0].slot, 1);
    }

    #[test]
    fn no_report_when_no_slot_is_assigned_to_the_deck() {
        let mut show = Show { deck_bus: [DeckBus::Off, DeckBus::Off, DeckBus::Off, DeckBus::Off], preset_catalog: catalog(&["P0", "P1"]), ..Default::default() };
        show.navigate_preset(Deck::A, 1);
        let out = show.take_fired_presets();
        assert!(out.is_empty());
    }

    #[test]
    fn drains_are_empty_after_the_first_call() {
        let mut show = Show { preset_catalog: catalog(&["P0", "P1"]), ..Default::default() };
        show.navigate_preset(Deck::A, 1);
        let first = show.take_fired_presets();
        assert_eq!(first.len(), 1);
        let second = show.take_fired_presets();
        assert!(second.is_empty());
    }

    #[test]
    fn reports_both_decks_when_both_have_fired() {
        let mut show = Show { preset_catalog: catalog(&["P0", "P1"]), ..Default::default() };
        show.navigate_preset(Deck::A, 1);
        show.navigate_preset(Deck::B, 1);
        let out = show.take_fired_presets();
        assert_eq!(out.len(), 2);
    }
}
