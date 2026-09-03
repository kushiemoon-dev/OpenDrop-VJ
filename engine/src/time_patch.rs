//! The Time panel's half of the host-to-preset side channel: which slot of
//! `preset_patch`'s packed word carries each of the 8 per-deck Time
//! multipliers, and which Milkdrop variable each one drives.
//!
//! # Index allocation
//!
//! The side channel is one `projectm_set_fps` word **per projectM instance**,
//! and this app gives every deck its own instance ([`crate::deck::Deck`]).
//! Indices are therefore per-deck, not global: deck 0 and deck 3 both use
//! slots 1..=8 for their own Time params with no possibility of collision,
//! and there is no need for per-deck index ranges. The 32 `CommandId::Time*`
//! commands are 8 params x 4 decks, so they map onto 8 indices repeated
//! across 4 independent channels rather than onto 32 slots of one.
//!
//! | index | Time param | Milkdrop variable |
//! | --- | --- | --- |
//! | 1 | Speed | *(none: see below)* |
//! | 2 | Zoom | `zoom` (scaled around 1) |
//! | 3 | Rotation | `rot` |
//! | 4 | Warp | `warp` |
//! | 5 | Horizontal | `dx` |
//! | 6 | Vertical | `dy` |
//! | 7 | Stretch | `sx` and `sy` (both, scaled around 1) |
//! | 8 | Wave | `wave_a` |
//!
//! Slots 9..=40 belong to Qvar's 32 watches ([`crate::qvar_patch`], Step 9),
//! keeping the whole Time+Qvar allocation inside the 40 slots the spike
//! sized for. Both families compete for the same one word per deck per
//! frame; `app` schedules them together in one round-robin rather than one
//! each.
//!
//! # Why Speed has no Milkdrop target
//!
//! The web app scales `time` itself (`core::time_params::inject_time_params`
//! prepends `a.time = a.time * speedMult` to the preset's frame equations).
//! That has no equivalent here, measured against real libprojectM 4.1.6:
//!
//! - `preset_patch` appends its lines after the preset's own equations,
//!   because the application lines must run after the preset has computed
//!   `zoom`/`rot`/…; a `time` write there is too late for the preset's own
//!   equations to see it;
//! - `time` is a per-frame *input*, not a persistent per-frame register:
//!   writing 25000 to it produced 0, not 25000, on the following frame, so
//!   an appended write cannot carry over either;
//! - `per_frame_0` is never executed (libprojectM reads `per_frame_N` from 1
//!   upwards), so there is no free slot ahead of the preset's own equations.
//!
//! Reaching `time` would mean renumbering every preset's own `per_frame_N`
//! lines to open room at the front, a rewrite of every one of the 9795
//! presets in the reference library on every load, for one slider, changing
//! the placement the spike verified end to end. Out of proportion, so Speed
//! stays host-side state: it is stored, persisted, and fully addressable by
//! keyboard/MIDI/OSC/remote-ws/LFO like the other seven, it just has no
//! visual effect on the projectM decks. Index 1 stays reserved for it so the
//! param-to-slot mapping is a plain 1:1 and a future engine change has its
//! slot waiting. See `TIME-QVAR-SPIKE.md` for the mechanism this builds on.

use crate::preset_patch::{Apply, PatchTarget};
use opendrop_core::time_params::DeckTimeParams;

/// The 8 Time multipliers, in side-channel index order.
pub const TIME_PARAM_COUNT: usize = 8;

/// Side-channel index of Time param `param` (0-based, in the order of
/// [`param_values`]), or `None` for a param with no Milkdrop target. See
/// the module docs on Speed.
pub fn side_channel_index(param: usize) -> Option<u16> {
    match param {
        0 => None, // Speed: reserved slot 1, no reachable Milkdrop variable.
        1..=7 => Some(param as u16 + 1),
        _ => None,
    }
}

/// The 8 multipliers as a flat array in side-channel index order, so the
/// per-frame push loop can diff and round-robin them without repeating the
/// field order at each call site.
pub fn param_values(params: &DeckTimeParams) -> [f64; TIME_PARAM_COUNT] {
    [
        params.speed_mult,
        params.zoom_mult,
        params.rot_mult,
        params.warp_mult,
        params.dx_mult,
        params.dy_mult,
        params.stretch_mult,
        params.wave_mult,
    ]
}

/// The [`PatchTarget`]s to patch a preset with before loading it onto a deck
/// whose Time params are `params`. Each target's `initial` is that deck's
/// *current* value, so a preset loaded mid-set comes up already scaled
/// rather than flashing its unscaled look until the first side-channel write
/// lands.
pub fn targets(params: &DeckTimeParams) -> Vec<PatchTarget> {
    let scale = |index: u16, initial: f64, var: &str| PatchTarget {
        index,
        initial,
        apply: Apply::ScaleAroundOne(var.to_string()),
    };
    let mult = |index: u16, initial: f64, var: &str| PatchTarget {
        index,
        initial,
        apply: Apply::Multiply(var.to_string()),
    };
    vec![
        scale(2, params.zoom_mult, "zoom"),
        mult(3, params.rot_mult, "rot"),
        mult(4, params.warp_mult, "warp"),
        mult(5, params.dx_mult, "dx"),
        mult(6, params.dy_mult, "dy"),
        // One slider, two variables. See `preset_patch`'s shared-index
        // handling.
        scale(7, params.stretch_mult, "sx"),
        scale(7, params.stretch_mult, "sy"),
        mult(8, params.wave_mult, "wave_a"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn param_values_are_in_side_channel_index_order() {
        let params = DeckTimeParams {
            speed_mult: 0.1,
            zoom_mult: 0.2,
            rot_mult: 0.3,
            warp_mult: 0.4,
            dx_mult: 0.5,
            dy_mult: 0.6,
            stretch_mult: 0.7,
            wave_mult: 0.8,
        };
        assert_eq!(param_values(&params), [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8]);
    }

    #[test]
    fn speed_has_no_side_channel_index_and_the_rest_are_2_to_8() {
        assert_eq!(side_channel_index(0), None);
        for param in 1..TIME_PARAM_COUNT {
            assert_eq!(side_channel_index(param), Some(param as u16 + 1));
        }
        assert_eq!(side_channel_index(TIME_PARAM_COUNT), None);
    }

    #[test]
    fn every_target_index_matches_its_params_own_slot() {
        // The push loop sends `set_param(side_channel_index(p), values[p])`,
        // so a target whose index disagreed with its param's slot would latch
        // the wrong register forever. Each param gets a *distinct* value on
        // purpose: this resolves a target back to its param through its
        // `initial`, which only works while no two params share a value;
        // `DeckTimeParams::default()` (all 1.0) would make every lookup
        // resolve to param 0 and the assertion vacuous.
        let params = DeckTimeParams {
            speed_mult: 0.1,
            zoom_mult: 0.2,
            rot_mult: 0.3,
            warp_mult: 0.4,
            dx_mult: 0.5,
            dy_mult: 0.6,
            stretch_mult: 0.7,
            wave_mult: 0.8,
        };
        let values = param_values(&params);
        let mut distinct = values.to_vec();
        distinct.sort_by(f64::total_cmp);
        distinct.dedup();
        assert_eq!(distinct.len(), TIME_PARAM_COUNT, "fixture values must stay distinct");
        for target in targets(&params) {
            let param = values
                .iter()
                .position(|v| (v - target.initial).abs() < 1e-12)
                .expect("every target's initial comes from one of the 8 params");
            assert_eq!(side_channel_index(param), Some(target.index));
        }
    }

    #[test]
    fn targets_stay_inside_the_40_slots_reserved_for_time_and_qvar() {
        for target in targets(&DeckTimeParams::default()) {
            assert!((2..=8).contains(&target.index), "{target:?} is outside Time's range");
        }
    }

    #[test]
    fn stretch_drives_both_axes_from_one_slot() {
        let params = DeckTimeParams { stretch_mult: 0.5, ..DeckTimeParams::default() };
        let stretch: Vec<_> = targets(&params).into_iter().filter(|t| t.index == 7).collect();
        assert_eq!(stretch.len(), 2);
        assert_eq!(stretch[0].apply, Apply::ScaleAroundOne("sx".to_string()));
        assert_eq!(stretch[1].apply, Apply::ScaleAroundOne("sy".to_string()));
        assert!(stretch.iter().all(|t| t.initial == 0.5));
    }

    #[test]
    fn zoom_and_stretch_scale_around_one_while_the_rest_multiply() {
        let by_index: std::collections::HashMap<u16, Apply> =
            targets(&DeckTimeParams::default()).into_iter().map(|t| (t.index, t.apply)).collect();
        assert_eq!(by_index[&2], Apply::ScaleAroundOne("zoom".to_string()));
        assert_eq!(by_index[&3], Apply::Multiply("rot".to_string()));
        assert_eq!(by_index[&4], Apply::Multiply("warp".to_string()));
        assert_eq!(by_index[&5], Apply::Multiply("dx".to_string()));
        assert_eq!(by_index[&6], Apply::Multiply("dy".to_string()));
        assert_eq!(by_index[&8], Apply::Multiply("wave_a".to_string()));
    }

    #[test]
    fn defaults_bake_in_neutral_initials() {
        for target in targets(&DeckTimeParams::default()) {
            assert_eq!(target.initial, 1.0);
        }
    }
}
