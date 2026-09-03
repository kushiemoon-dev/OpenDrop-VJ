//! The Qvar panel's half of the host-to-preset side channel: which slot of
//! `preset_patch`'s packed word carries each of the 32 per-deck q-var
//! overrides, and how they reach the Milkdrop `q1`..`q32` registers.
//!
//! # Index allocation
//!
//! Indices are per-deck, for the same reason they are in
//! [`crate::time_patch`]: the channel is one `projectm_set_fps` word **per
//! projectM instance**, and every deck has its own instance. The 128
//! `CommandId::Qvar*` commands are 32 watches x 4 decks, so they map onto 32
//! indices repeated across 4 independent channels, not onto 128 slots of one.
//!
//! | index | carries |
//! | --- | --- |
//! | 1..=8 | Time (see [`crate::time_patch`]) |
//! | 9..=40 | q-var 1..=32 of this deck |
//!
//! Slots 41..=999 stay free.
//!
//! # Why the *set* of watches costs a reload and a value does not
//!
//! `preset_patch` bakes its application lines into the preset text at load
//! time, so a q-var that is not a [`PatchTarget`] when the preset is loaded
//! has no `q{n} = od_p{i};` line and no register: writing its index into the
//! channel afterwards latches nothing. Enabling or disabling a watch
//! therefore means re-patching and reloading that deck's preset (`app`'s
//! `resync_deck_q_var_watches`), which restarts the preset's own animation
//! exactly as loading any other preset does. Changing a *watched value*
//! costs nothing of the sort: it rides the per-frame channel like Time's
//! multipliers.
//!
//! That is a deliberate departure from the web reference, whose
//! `core::q_vars::inject_q_var_params` emits all 32 guard lines
//! unconditionally and re-reads `enabled` from a JS object every frame, a
//! host-owned object a Milkdrop preset has no way to see. The equivalent
//! here would be a second index range carrying 32 enable flags plus a gated
//! application line per q-var, i.e. ~160 extra equation lines appended to
//! *every* preset on *every* deck, whether or not the user ever opens the
//! panel, in exchange for removing a glitch on an explicit, occasional
//! click. The reload is the cheaper half of that trade; if watch toggling
//! ever becomes something an LFO does continuously, revisit it.
//!
//! # Why `Apply::Assign`
//!
//! A q-var has no documented meaning and so no neutral value (see
//! `core::q_vars`): presets use `q1`..`q32` as scratch registers for
//! whatever they like. The web port overrides them outright (`q{n} =
//! value`), so this does too: [`Apply::Assign`], not
//! [`Apply::Multiply`]/[`Apply::ScaleAroundOne`], which would need a neutral
//! element that does not exist here. The application lines land at the end
//! of `per_frame`, so an override wins over whatever the preset computed for
//! that q-var that frame, and loses to a `per_pixel` block that recomputes
//! it. The same caveat the web port carries.

use crate::preset_patch::{Apply, PatchTarget};
use opendrop_core::q_vars::{DeckQVarParams, Q_VAR_COUNT};

/// How many q-var watches a deck has: one per Milkdrop `q1`..`q32`.
pub const QVAR_WATCH_COUNT: usize = Q_VAR_COUNT;

/// First side-channel index this module claims. Immediately after Time's
/// 1..=8 (see [`crate::time_patch`]), so the two families never collide on
/// the deck channel they share.
pub const QVAR_INDEX_BASE: u16 = 9;

/// Side-channel index of q-var watch `watch` (0-based, i.e. `q(watch + 1)`),
/// or `None` when `watch` is out of range.
pub fn side_channel_index(watch: usize) -> Option<u16> {
    (watch < QVAR_WATCH_COUNT).then(|| QVAR_INDEX_BASE + watch as u16)
}

/// The [`PatchTarget`]s to patch a preset with before loading it onto a deck
/// whose q-var overrides are `params`: one per *enabled* watch, none for the
/// disabled ones: a disabled watch must leave the preset's own `q{n}`
/// alone, and the only way to express that is to emit no line for it.
///
/// Each target's `initial` is that watch's *current* value, so a preset
/// loaded mid-set comes up already overridden rather than showing its own
/// q-var for a frame until the first side-channel write lands. Same
/// reasoning as `time_patch::targets`.
pub fn targets(params: &DeckQVarParams) -> Vec<PatchTarget> {
    (0..QVAR_WATCH_COUNT)
        .filter(|&watch| params.enabled[watch])
        .filter_map(|watch| {
            Some(PatchTarget {
                index: side_channel_index(watch)?,
                initial: params.value[watch],
                apply: Apply::Assign(format!("q{}", watch + 1)),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use opendrop_core::q_vars::default_q_var_params;

    fn watching(watches: &[(usize, f64)]) -> DeckQVarParams {
        let mut params = default_q_var_params();
        for &(watch, value) in watches {
            params.enabled[watch] = true;
            params.value[watch] = value;
        }
        params
    }

    #[test]
    fn indices_run_from_9_to_40_leaving_times_1_to_8_alone() {
        assert_eq!(side_channel_index(0), Some(9));
        assert_eq!(side_channel_index(QVAR_WATCH_COUNT - 1), Some(40));
        assert_eq!(side_channel_index(QVAR_WATCH_COUNT), None);
    }

    #[test]
    fn no_index_collides_with_any_time_target() {
        // The two families share one word per deck per frame, so an index
        // used by both would make a Time slider silently move a q-var.
        let time: Vec<u16> = crate::time_patch::targets(&Default::default()).into_iter().map(|t| t.index).collect();
        for watch in 0..QVAR_WATCH_COUNT {
            let index = side_channel_index(watch).expect("in range");
            assert!(!time.contains(&index), "watch {watch} collides with a Time slot");
            assert!((9..=40).contains(&index));
        }
    }

    #[test]
    fn a_deck_with_no_watches_patches_nothing() {
        assert_eq!(targets(&default_q_var_params()), Vec::new());
    }

    #[test]
    fn only_enabled_watches_become_targets() {
        // A disabled watch keeps its last value (`without_q_var_watch`
        // leaves it alone on purpose), and emitting a line for it would
        // clobber the preset's own q-var with a value the user has switched
        // off.
        let mut params = watching(&[(0, 1.5), (6, -0.25)]);
        params.value[9] = 1.75; // disabled, but not zero
        let indices: Vec<u16> = targets(&params).into_iter().map(|t| t.index).collect();
        assert_eq!(indices, vec![9, 15]);
    }

    #[test]
    fn each_target_assigns_its_own_q_var_at_its_own_index() {
        let params = watching(&[(0, 0.0), (6, 0.0), (31, 0.0)]);
        let mapped: Vec<(u16, Apply)> = targets(&params).into_iter().map(|t| (t.index, t.apply)).collect();
        assert_eq!(
            mapped,
            vec![
                (9, Apply::Assign("q1".to_string())),
                (15, Apply::Assign("q7".to_string())),
                (40, Apply::Assign("q32".to_string())),
            ]
        );
    }

    #[test]
    fn bakes_the_current_value_in_as_the_presets_starting_point() {
        let targets = targets(&watching(&[(3, -1.75)]));
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].initial, -1.75);
    }

    #[test]
    fn every_watch_at_once_produces_32_distinct_targets() {
        let params = watching(&(0..QVAR_WATCH_COUNT).map(|w| (w, 0.0)).collect::<Vec<_>>());
        let targets = targets(&params);
        assert_eq!(targets.len(), QVAR_WATCH_COUNT);
        let mut indices: Vec<u16> = targets.iter().map(|t| t.index).collect();
        indices.dedup();
        assert_eq!(indices.len(), QVAR_WATCH_COUNT);
    }

    #[test]
    fn patched_output_assigns_the_q_var_from_its_own_register() {
        // The end-to-end shape this module exists to produce, asserted here
        // rather than only in `preset_patch`'s own tests: index 15 latches
        // and q7 reads it.
        let out = crate::preset_patch::patch_preset(
            "per_frame_1=q7 = 0.5;\n",
            &targets(&watching(&[(6, 1.25)])),
            crate::preset_patch::MEASURED_DEFAULT_FPS,
        );
        assert!(out.contains("per_frame_init_1=od_p15 = 1.25;"), "{out}");
        assert!(out.contains("od_p15 = equal(od_i,15)*od_v + (1-equal(od_i,15))*od_p15;"), "{out}");
        assert!(out.contains("q7 = od_p15;"), "{out}");
        // The preset's own q7 line still runs first: the override is
        // appended after it, not instead of it.
        assert!(out.find("q7 = 0.5;").unwrap() < out.find("q7 = od_p15;").unwrap(), "{out}");
    }
}
