//! Live show state driving the compositor: crossfader, per-deck bus
//! assignment, per-slot composite config, per-bus color params. Pure
//! state/logic: no GL, no I/O. Implements `commands::CommandContext` so
//! the keyboard dispatch (`app::keymap` + `commands::create_default_registry`)
//! can drive it directly.
//!
//! `bus_gain` and the default bus assignment are ported from OpenDrop-VJ
//! `src/routes/+page.svelte:264-269`.

use crate::blend::{ColorParams, SlotComposite, DEFAULT_COLOR_PARAMS, DEFAULT_SLOT_COMPOSITE};
use crate::commands::{CommandContext, Deck};

/// Which side of the crossfader a deck slot is assigned to. `Off` means the
/// slot never shows, regardless of crossfader position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeckBus {
    A,
    B,
    Off,
}

/// `busGain` from `+page.svelte:264-268`: `A -> 1-x`, `B -> x`, `off -> 0`.
pub fn bus_gain(bus: DeckBus, x: f64) -> f64 {
    match bus {
        DeckBus::A => 1.0 - x,
        DeckBus::B => x,
        DeckBus::Off => 0.0,
    }
}

impl DeckBus {
    /// Cycles the bus assignment: `A -> B -> Off -> A`.
    pub fn next(self) -> Self {
        match self {
            DeckBus::A => DeckBus::B,
            DeckBus::B => DeckBus::Off,
            DeckBus::Off => DeckBus::A,
        }
    }
}

pub struct Show {
    pub crossfader: f64,
    pub deck_bus: [DeckBus; 4],
    pub active_deck: Deck,
    pub selected_slot: usize,
    pub slot_composites: [SlotComposite; 4],
    pub color_params_a: ColorParams,
    pub color_params_b: ColorParams,
}

impl Default for Show {
    fn default() -> Self {
        Self {
            crossfader: 0.0,
            deck_bus: [DeckBus::A, DeckBus::B, DeckBus::Off, DeckBus::Off],
            active_deck: Deck::A,
            selected_slot: 0,
            slot_composites: [DEFAULT_SLOT_COMPOSITE; 4],
            color_params_a: DEFAULT_COLOR_PARAMS,
            color_params_b: DEFAULT_COLOR_PARAMS,
        }
    }
}

impl Show {
    /// Per-slot opacity for the compositor: `bus_gain(deck_bus[slot], crossfader)`.
    pub fn slot_opacities(&self) -> [f64; 4] {
        std::array::from_fn(|i| bus_gain(self.deck_bus[i], self.crossfader))
    }

    /// Selects a physical slot (clicked deck-card) and derives `active_deck`
    /// from its bus assignment. Port of `activeDeckLetter` in
    /// `MixerLayout.svelte:62`: `Off` falls back to `A`, same as the
    /// original ternary.
    pub fn select_slot(&mut self, slot: usize) {
        self.selected_slot = slot;
        self.active_deck = if self.deck_bus[slot] == DeckBus::B { Deck::B } else { Deck::A };
    }

    /// Resolves a bus letter to the first physical slot assigned to it.
    pub fn deck_bus_slot_for(&self, deck: Deck) -> Option<usize> {
        self.deck_bus.iter().position(|&b| match deck {
            Deck::A => b == DeckBus::A,
            Deck::B => b == DeckBus::B,
        })
    }
}

impl CommandContext for Show {
    fn get_crossfader(&self) -> f64 {
        self.crossfader
    }

    fn set_crossfader(&mut self, v: f64) {
        self.crossfader = v.clamp(0.0, 1.0);
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

    // Preset browsing, playlists, and the overlay queue are Phase 4/M2+
    // territory (see commands.rs's own header note: most CommandId
    // variants are no-op stubs in the TS source too, wired up by later
    // milestones). Phase 2 has no preset list or overlay queue to act on.
    fn navigate_preset(&mut self, _deck: Deck, _direction: i32) {}
    fn toggle_playlist(&mut self, _deck: Deck) {}
    fn playlist_next(&mut self, _deck: Deck) {}
    fn playlist_prev(&mut self, _deck: Deck) {}
    fn advance_overlay_queue(&mut self, _direction: i32) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{create_default_registry, CommandId};

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
    }

    mod slot_opacities {
        use super::*;

        #[test]
        fn default_state_is_full_a_zero_elsewhere() {
            assert_eq!(Show::default().slot_opacities(), [1.0, 0.0, 0.0, 0.0]);
        }

        #[test]
        fn crossfader_at_one_is_full_b_zero_elsewhere() {
            let mut show = Show::default();
            show.crossfader = 1.0;
            assert_eq!(show.slot_opacities(), [0.0, 1.0, 0.0, 0.0]);
        }

        #[test]
        fn crossfader_at_midpoint_splits_a_and_b_evenly() {
            let mut show = Show::default();
            show.crossfader = 0.5;
            assert_eq!(show.slot_opacities(), [0.5, 0.5, 0.0, 0.0]);
        }

        #[test]
        fn off_slots_never_move() {
            let mut show = Show::default();
            show.crossfader = 1.0;
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
            let mut show = Show::default();
            show.deck_bus = [DeckBus::Off, DeckBus::Off, DeckBus::Off, DeckBus::Off];
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
}
