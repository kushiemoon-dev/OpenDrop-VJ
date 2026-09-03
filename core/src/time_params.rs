//! Port of OpenDrop-VJ `src/lib/engine/time-params.ts`: the 8 per-deck time
//! multipliers behind the `time-speed`/`time-zoom`/`time-rot`/`time-warp`/
//! `time-dx`/`time-dy`/`time-stretch`/`time-wave` command families in
//! `commands.rs` (currently no-op stubs pending this module).
//!
//! `getGlobalTimeParams()` from the TS source is intentionally not ported:
//! it lazily inits and returns a `window` global for Butterchurn's compiled
//! preset code to read, and the TS file's own doc comment notes it is "not
//! unit tested: touches window". That's I/O-shaped runtime glue, not pure
//! logic; ownership of the equivalent global state belongs in a future
//! GPU/runtime-facing crate, not here.

/// Upper bound of every Time multiplier: the panel's sliders run 0-2 in
/// steps of 0.01 (`SidebarTime.svelte`), 1 being neutral. Also the factor a
/// `CommandId::Time*` dispatch's 0..1 value is scaled by, so a MIDI fader at
/// half travel lands exactly on neutral.
pub const TIME_MULT_MAX: f64 = 2.0;

/// Clamps a multiplier to the panel's 0..[`TIME_MULT_MAX`] range: the same
/// role `set_crossfader`'s own `clamp(0.0, 1.0)` plays for the crossfader.
pub fn clamp_time_mult(v: f64) -> f64 {
    v.clamp(0.0, TIME_MULT_MAX)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeckTimeParams {
    pub speed_mult: f64,
    pub zoom_mult: f64,
    pub rot_mult: f64,
    pub warp_mult: f64,
    pub dx_mult: f64,
    pub dy_mult: f64,
    pub stretch_mult: f64,
    pub wave_mult: f64,
}

/// Neutral (no-op) multipliers: 1.0, not 0.0, since these scale existing values.
impl Default for DeckTimeParams {
    fn default() -> Self {
        Self {
            speed_mult: 1.0,
            zoom_mult: 1.0,
            rot_mult: 1.0,
            warp_mult: 1.0,
            dx_mult: 1.0,
            dy_mult: 1.0,
            stretch_mult: 1.0,
            wave_mult: 1.0,
        }
    }
}

/// Time params for the 4 decks, indexed 0-3.
pub type TimeParamsTuple = [DeckTimeParams; 4];

/// Partial update for [`with_time_params`]: mirrors TS `Partial<DeckTimeParams>`.
/// Unset (`None`) fields leave the existing value untouched.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DeckTimeParamsPatch {
    pub speed_mult: Option<f64>,
    pub zoom_mult: Option<f64>,
    pub rot_mult: Option<f64>,
    pub warp_mult: Option<f64>,
    pub dx_mult: Option<f64>,
    pub dy_mult: Option<f64>,
    pub stretch_mult: Option<f64>,
    pub wave_mult: Option<f64>,
}

/// Merge `patch` into one slot. Pure: returns a new tuple; does not write
/// through to any global state (see module docs re: `getGlobalTimeParams`).
pub fn with_time_params(
    params: &TimeParamsTuple,
    slot: usize,
    patch: DeckTimeParamsPatch,
) -> TimeParamsTuple {
    let mut next = *params;
    let current = next[slot];
    next[slot] = DeckTimeParams {
        speed_mult: patch.speed_mult.unwrap_or(current.speed_mult),
        zoom_mult: patch.zoom_mult.unwrap_or(current.zoom_mult),
        rot_mult: patch.rot_mult.unwrap_or(current.rot_mult),
        warp_mult: patch.warp_mult.unwrap_or(current.warp_mult),
        dx_mult: patch.dx_mult.unwrap_or(current.dx_mult),
        dy_mult: patch.dy_mult.unwrap_or(current.dy_mult),
        stretch_mult: patch.stretch_mult.unwrap_or(current.stretch_mult),
        wave_mult: patch.wave_mult.unwrap_or(current.wave_mult),
    };
    next
}

/// Builds the patched `frame_eqs_str` Butterchurn's compiled preset code reads
/// multipliers from via `window.__odDeckParams[slot].*Mult`. The 8 lines are
/// injected unconditionally on every call: no "which variables are active"
/// state to track, every slider stays live-adjustable without a preset reload.
///
/// Takes the preset's existing `frame_eqs_str` directly rather than a whole
/// preset object: `q_vars.rs`'s `Preset` (the crate's only `Preset` type so
/// far) is explicitly a minimal stand-in scoped to `inject_q_var_params`'s
/// own `frame_eqs_str`-only patching, not a general preset representation,
/// so splicing this back into a real preset stays whichever future
/// milestone's concern assembles one, not this pure string transform's.
pub fn inject_time_params(frame_eqs_str: &str, slot: usize) -> String {
    let p = format!("window.__odDeckParams[{slot}]");
    format!(
        "a.time = a.time * {p}.speedMult;\n\
         {frame_eqs_str}\n\
         a.zoom = 1 + (a.zoom - 1) * {p}.zoomMult;\n\
         a.rot = a.rot * {p}.rotMult;\n\
         a.warp = a.warp * {p}.warpMult;\n\
         a.dx = a.dx * {p}.dxMult;\n\
         a.dy = a.dy * {p}.dyMult;\n\
         a.sx = 1 + (a.sx - 1) * {p}.stretchMult;\n\
         a.sy = 1 + (a.sy - 1) * {p}.stretchMult;\n\
         a.wave_a = a.wave_a * {p}.waveMult;"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    mod default_time_params {
        use super::*;

        #[test]
        fn all_multipliers_equal_one_neutral() {
            assert_eq!(
                DeckTimeParams::default(),
                DeckTimeParams {
                    speed_mult: 1.0,
                    zoom_mult: 1.0,
                    rot_mult: 1.0,
                    warp_mult: 1.0,
                    dx_mult: 1.0,
                    dy_mult: 1.0,
                    stretch_mult: 1.0,
                    wave_mult: 1.0,
                }
            );
        }
    }

    mod inject_time_params {
        use super::*;

        #[test]
        fn does_not_mutate_the_original_string() {
            let original = String::from("a.zoom = 1.01;");
            let patched = inject_time_params(&original, 0);
            assert_eq!(original, "a.zoom = 1.01;");
            assert_ne!(patched, original);
        }

        #[test]
        fn prefixes_scaled_a_time_before_the_original_code() {
            let patched = inject_time_params("a.zoom = 1.01;", 0);
            let speed_line_index = patched.find("a.time = a.time *");
            let original_line_index = patched.find("a.zoom = 1.01;");
            assert!(speed_line_index.is_some());
            assert!(speed_line_index < original_line_index);
        }

        #[test]
        fn adds_the_7_multiplier_lines_referencing_the_slot() {
            let patched = inject_time_params("a.zoom = 1.01;", 2);
            for field in [
                "zoomMult",
                "rotMult",
                "warpMult",
                "dxMult",
                "dyMult",
                "stretchMult",
                "waveMult",
            ] {
                assert!(patched.contains(&format!("window.__odDeckParams[2].{field}")));
            }
        }

        #[test]
        fn namespaces_correctly_per_slot_no_collision_between_decks() {
            let patched0 = inject_time_params("", 0);
            let patched3 = inject_time_params("", 3);
            assert!(patched0.contains("window.__odDeckParams[0]"));
            assert!(!patched0.contains("window.__odDeckParams[3]"));
            assert!(patched3.contains("window.__odDeckParams[3]"));
            assert!(!patched3.contains("window.__odDeckParams[0]"));
        }

        #[test]
        fn handles_an_empty_frame_eqs_str() {
            let patched = inject_time_params("", 0);
            assert!(patched.contains("a.time = a.time *"));
        }
    }

    mod with_time_params {
        use super::*;

        fn make_params() -> TimeParamsTuple {
            [DeckTimeParams::default(); 4]
        }

        #[test]
        fn updates_only_the_targeted_slot() {
            let params = make_params();
            let next = with_time_params(
                &params,
                1,
                DeckTimeParamsPatch { speed_mult: Some(1.5), ..Default::default() },
            );
            assert_eq!(next[1].speed_mult, 1.5);
            assert_eq!(next[0].speed_mult, 1.0);
            assert_eq!(next[2].speed_mult, 1.0);
        }

        #[test]
        fn merges_a_partial_patch_without_touching_other_fields() {
            let params = make_params();
            let next = with_time_params(
                &params,
                0,
                DeckTimeParamsPatch { zoom_mult: Some(2.0), ..Default::default() },
            );
            assert_eq!(next[0].zoom_mult, 2.0);
            assert_eq!(next[0].speed_mult, 1.0);
        }

        #[test]
        fn does_not_mutate_the_array_or_the_source_objects() {
            let params = make_params();
            let next = with_time_params(
                &params,
                3,
                DeckTimeParamsPatch { rot_mult: Some(0.5), ..Default::default() },
            );
            assert_ne!(next[3], params[3]);
            assert_eq!(params[3].rot_mult, 1.0);
        }
    }
}
