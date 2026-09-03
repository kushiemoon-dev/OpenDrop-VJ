//! Port of OpenDrop-VJ `src/lib/engine/q-vars.ts`, Q-var live editing (Track 2):
//! generic q1-q32 overrides per deck, matching NestDrop's "Q Var" knobs. Presets
//! define/use these internally with no universal meaning (unlike Time's
//! documented speed/zoom/etc, see time_params.rs), so there is no neutral
//! numeric default. The override is opt-in per slot via `enabled`, checked live
//! every frame by the compiled preset code rather than baked in at compile
//! time: this is what lets toggling a q-var on/off take effect without
//! reloading the preset.
//!
//! That last sentence is about Butterchurn, and does **not** carry over to
//! the native projectM path: a Milkdrop preset cannot read a host-side
//! `enabled` flag every frame, so `engine::qvar_patch` compiles only the
//! watches that are on into the preset text, and toggling one costs that
//! deck a reload. Values still change without one. See that module for why
//! the trade was made that way round.
//!
//! `getGlobalQVarParams` (the `window.__odQVarParams` singleton) is UI/runtime
//! glue over a global mutable binding, untested in the TS source itself, and
//! out of scope for this pure-logic port.

use std::collections::HashMap;

/// How many q-vars a preset exposes (`q1`..`q32`), and so the width of both
/// [`DeckQVarParams`] arrays. Named for the callers that iterate them, the
/// engine's side-channel index allocation (`engine::qvar_patch`) and the Qvar
/// panel, rather than repeating `32` at each of them.
pub const Q_VAR_COUNT: usize = 32;

/// Lowest value a q-var override can take: `SidebarQvar.svelte`'s sliders run
/// -2..2 in steps of 0.01. Also the range a `CommandId::Qvar*` dispatch's
/// 0..1 value is scaled onto, so a MIDI fader at half travel lands exactly on
/// 0.
pub const Q_VAR_MIN: f64 = -2.0;

/// Highest value a q-var override can take: see [`Q_VAR_MIN`].
pub const Q_VAR_MAX: f64 = 2.0;

/// Clamps an override to the panel's [`Q_VAR_MIN`]..[`Q_VAR_MAX`] range, the
/// same role `time_params::clamp_time_mult` plays for the Time multipliers.
/// That range is also exactly what the host-to-preset side channel can carry
/// (`engine::preset_patch::VALUE_MIN`/`VALUE_MAX`), so a clamped value always
/// survives the trip into the running preset unaltered.
pub fn clamp_q_var_value(v: f64) -> f64 {
    v.clamp(Q_VAR_MIN, Q_VAR_MAX)
}

/// Q-var overrides for one deck. `enabled[i]`/`value[i]` correspond to q(i+1).
/// Fixed at 32 slots: the length invariant the TS source enforced by
/// convention is enforced here by the type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeckQVarParams {
    pub enabled: [bool; 32],
    pub value: [f64; 32],
}

pub fn default_q_var_params() -> DeckQVarParams {
    DeckQVarParams { enabled: [false; 32], value: [0.0; 32] }
}

/// Q-var params for the 4 decks, indexed 0-3.
pub type QVarParamsTuple = [DeckQVarParams; 4];

/// Update a single q-var's value (1-indexed) for one deck slot, without
/// touching `enabled`. Pure: returns a new tuple. Whole-branch review
/// Finding M5: an out-of-range `slot` (>3) or `n` (0, or >32) used to panic
/// via array indexing; both are now a no-op (the input tuple comes back
/// unchanged) instead, the least invasive fix that doesn't change this
/// function's `QVarParamsTuple -> QVarParamsTuple` contract into a
/// `Result`/`Option`.
pub fn with_q_var_value(
    mut params: QVarParamsTuple,
    slot: usize,
    n: usize,
    value: f64,
) -> QVarParamsTuple {
    if slot >= params.len() || n == 0 || n > 32 {
        return params;
    }
    params[slot].value[n - 1] = value;
    params
}

/// Enable watching a q-var (1-indexed), resetting its value to 0. Pure:
/// returns a new tuple. See `with_q_var_value`'s doc comment re: out-of-range
/// `slot`/`n` (Finding M5).
pub fn with_q_var_watch(mut params: QVarParamsTuple, slot: usize, n: usize) -> QVarParamsTuple {
    if slot >= params.len() || n == 0 || n > 32 {
        return params;
    }
    params[slot].enabled[n - 1] = true;
    params[slot].value[n - 1] = 0.0;
    params
}

/// Disable watching a q-var (1-indexed), leaving its last value untouched.
/// Pure: returns a new tuple. See `with_q_var_value`'s doc comment re:
/// out-of-range `slot`/`n` (Finding M5).
pub fn without_q_var_watch(mut params: QVarParamsTuple, slot: usize, n: usize) -> QVarParamsTuple {
    if slot >= params.len() || n == 0 || n > 32 {
        return params;
    }
    params[slot].enabled[n - 1] = false;
    params
}

/// Minimal stand-in for a Butterchurn/Milkdrop preset: only `frame_eqs_str` is
/// interpreted by `inject_q_var_params`, every other field is opaque and must
/// pass through untouched.
pub type Preset = HashMap<String, String>;

/// Shallow-clones `preset` and appends 32 guard lines to its frame_eqs_str,
/// each referencing window.__odQVarParams[slot]: pure string manipulation.
/// All 32 lines are always emitted; the `enabled` check runs at preset-eval
/// time every frame, not here: this is what lets enabling/disabling a q-var
/// take effect without recompiling the preset.
pub fn inject_q_var_params(preset: &Preset, slot: usize) -> Preset {
    let mut patched = preset.clone();
    let original = patched.get("frame_eqs_str").cloned().unwrap_or_default();
    let prefix = format!("window.__odQVarParams[{slot}]");
    let guards = (0..32)
        .map(|i| format!("if ({prefix}.enabled[{i}]) {{ q{n} = {prefix}.value[{i}]; }}", n = i + 1))
        .collect::<Vec<_>>()
        .join("\n");
    patched.insert("frame_eqs_str".to_string(), format!("{original}\n{guards}"));
    patched
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_q_var_params_has_32_slots_all_disabled_value_0() {
        let p = default_q_var_params();
        assert_eq!(p.enabled.len(), 32);
        assert_eq!(p.value.len(), 32);
        assert!(p.enabled.iter().all(|&e| !e));
        assert!(p.value.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn q_var_count_matches_the_array_width_it_names() {
        let p = default_q_var_params();
        assert_eq!(Q_VAR_COUNT, p.enabled.len());
        assert_eq!(Q_VAR_COUNT, p.value.len());
    }

    #[test]
    fn clamp_q_var_value_holds_the_sliders_own_range() {
        assert_eq!(clamp_q_var_value(9.0), Q_VAR_MAX);
        assert_eq!(clamp_q_var_value(-9.0), Q_VAR_MIN);
        assert_eq!(clamp_q_var_value(0.75), 0.75);
    }

    mod inject_q_var_params_tests {
        use super::*;

        fn preset_with(pairs: &[(&str, &str)]) -> Preset {
            pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
        }

        #[test]
        fn clones_the_preset_without_mutating_the_original() {
            let preset = preset_with(&[("frame_eqs_str", "a.zoom = 1.01;"), ("other", "field")]);
            let patched = inject_q_var_params(&preset, 0);
            assert_eq!(preset.get("frame_eqs_str").unwrap(), "a.zoom = 1.01;");
            assert_eq!(patched.get("other").unwrap(), "field");
        }

        #[test]
        fn adds_the_original_code_before_the_32_guard_lines() {
            let preset = preset_with(&[("frame_eqs_str", "a.zoom = 1.01;")]);
            let patched = inject_q_var_params(&preset, 0);
            let frame_eqs_str = patched.get("frame_eqs_str").unwrap();
            let original_index = frame_eqs_str.find("a.zoom = 1.01;").unwrap();
            let first_guard_index =
                frame_eqs_str.find("if (window.__odQVarParams[0].enabled[0])").unwrap();
            assert!(first_guard_index > original_index);
        }

        #[test]
        fn generates_32_guard_lines_q1_to_q32_referencing_window_odqvarparams_slot() {
            let preset = preset_with(&[("frame_eqs_str", "")]);
            let patched = inject_q_var_params(&preset, 2);
            let frame_eqs_str = patched.get("frame_eqs_str").unwrap();
            for n in 1..=32 {
                let expected = format!(
                    "if (window.__odQVarParams[2].enabled[{}]) {{ q{n} = window.__odQVarParams[2].value[{}]; }}",
                    n - 1,
                    n - 1
                );
                assert!(frame_eqs_str.contains(&expected));
            }
        }

        #[test]
        fn namespaces_correctly_per_slot_no_collision_between_decks() {
            let preset = preset_with(&[("frame_eqs_str", "")]);
            let patched0 = inject_q_var_params(&preset, 0);
            let patched3 = inject_q_var_params(&preset, 3);
            let f0 = patched0.get("frame_eqs_str").unwrap();
            let f3 = patched3.get("frame_eqs_str").unwrap();
            assert!(f0.contains("window.__odQVarParams[0]"));
            assert!(!f0.contains("window.__odQVarParams[3]"));
            assert!(f3.contains("window.__odQVarParams[3]"));
            assert!(!f3.contains("window.__odQVarParams[0]"));
        }

        #[test]
        fn handles_a_preset_without_frame_eqs_str_empty_string_by_default() {
            let preset = Preset::new();
            let patched = inject_q_var_params(&preset, 0);
            assert!(patched
                .get("frame_eqs_str")
                .unwrap()
                .contains("if (window.__odQVarParams[0].enabled[0])"));
        }
    }

    mod with_q_var_value_tests {
        use super::*;

        fn params() -> QVarParamsTuple {
            [default_q_var_params(); 4]
        }

        #[test]
        fn updates_the_value_of_the_targeted_q_var_for_the_targeted_slot() {
            let next = with_q_var_value(params(), 1, 5, 1.5);
            assert_eq!(next[1].value[4], 1.5);
            assert_eq!(next[0].value[4], 0.0);
            assert_eq!(next[2].value[4], 0.0);
        }

        #[test]
        fn does_not_touch_enabled_or_the_other_values() {
            let next = with_q_var_value(params(), 0, 3, -1.0);
            assert!(!next[0].enabled[2]);
            assert_eq!(next[0].value[0], 0.0);
        }

        #[test]
        fn does_not_mutate_the_source_params() {
            let original = params();
            let _next = with_q_var_value(original, 2, 1, 2.0);
            assert_eq!(original[2].value[0], 0.0);
        }

        #[test]
        fn an_out_of_range_slot_is_a_no_op_instead_of_panicking() {
            let original = params();
            let next = with_q_var_value(original, 4, 1, 5.0); // slots are 0..=3
            assert_eq!(next, original);
        }

        #[test]
        fn n_0_is_a_no_op_instead_of_panicking() {
            let original = params();
            let next = with_q_var_value(original, 0, 0, 5.0); // n is 1-indexed
            assert_eq!(next, original);
        }

        #[test]
        fn n_above_32_is_a_no_op_instead_of_panicking() {
            let original = params();
            let next = with_q_var_value(original, 0, 33, 5.0);
            assert_eq!(next, original);
        }
    }

    mod with_q_var_watch_tests {
        use super::*;

        fn params() -> QVarParamsTuple {
            [default_q_var_params(); 4]
        }

        #[test]
        fn enables_the_watch_and_resets_the_value_to_0() {
            let dirty = with_q_var_value(params(), 0, 7, 1.9);
            let next = with_q_var_watch(dirty, 0, 7);
            assert!(next[0].enabled[6]);
            assert_eq!(next[0].value[6], 0.0);
        }

        #[test]
        fn does_not_touch_the_other_slots_q_vars() {
            let next = with_q_var_watch(params(), 1, 10);
            assert!(!next[0].enabled[9]);
            assert_eq!(next[1].enabled.iter().filter(|&&e| e).count(), 1);
        }

        #[test]
        fn an_out_of_range_slot_or_n_is_a_no_op_instead_of_panicking() {
            let original = params();
            assert_eq!(with_q_var_watch(original, 4, 1), original);
            assert_eq!(with_q_var_watch(original, 0, 0), original);
            assert_eq!(with_q_var_watch(original, 0, 33), original);
        }
    }

    mod without_q_var_watch_tests {
        use super::*;

        #[test]
        fn disables_the_watch_without_touching_the_last_value() {
            let params: QVarParamsTuple = [default_q_var_params(); 4];
            let watched = with_q_var_watch(params, 0, 12);
            let valued = with_q_var_value(watched, 0, 12, 1.2);
            let next = without_q_var_watch(valued, 0, 12);
            assert!(!next[0].enabled[11]);
            assert_eq!(next[0].value[11], 1.2);
        }

        #[test]
        fn an_out_of_range_slot_or_n_is_a_no_op_instead_of_panicking() {
            let original: QVarParamsTuple = [default_q_var_params(); 4];
            assert_eq!(without_q_var_watch(original, 4, 1), original);
            assert_eq!(without_q_var_watch(original, 0, 0), original);
            assert_eq!(without_q_var_watch(original, 0, 33), original);
        }
    }
}
