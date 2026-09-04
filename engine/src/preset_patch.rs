//! Host → running-preset numeric injection for projectM 4.
//!
//! # Why this exists
//!
//! Time (8 params/deck) and Qvar (32 watches/deck) both need the host to feed
//! a number into a preset that is already rendering, continuously: a slider
//! dragged live, or the LFO engine ticking every frame. libprojectM 4.1.6
//! exports 47 C functions and not one of them addresses a preset-internal
//! variable (see `.planning/TIME-QVAR-SPIKE.md` for the full audit).
//! The obvious fallback (patch the preset text and call
//! `projectm_load_preset_data` every time the value changes) was measured and
//! rejected: a reload costs 3.5-9.2 ms (3-10x a whole rendered frame) and, far
//! worse, re-runs `per_frame_init`, so every accumulator the preset integrates
//! frame over frame (`basstime = basstime + bass*0.005`, `tt = tt + tic*treb`)
//! snaps back to zero at the reload rate. Continuous modulation by reload is
//! not possible.
//!
//! # The mechanism
//!
//! `projectm_set_fps` is the one host-writable value that reaches a running
//! preset. Its own header says the value is "passed on to presets, which may
//! choose to use it for calculations. It is not used in any other way by the
//! library". projectM never reads it back, it only forwards it to the
//! Milkdrop `fps` variable, live, with no reload. That makes it a free 32-bit
//! word per projectM instance (so: per deck), writable once per frame.
//!
//! So: patch the preset text **once, at load time** with a small demux
//! prologue that unpacks that word into persistent per-frame registers, then
//! write one `(index, value)` pair per frame with `Deck::set_param`. The
//! registers are ordinary Milkdrop per-frame variables, so they hold their
//! value between frames and the preset's own animation state is never
//! disturbed.
//!
//! Wire format, verified end to end against libprojectM 4.1.6 on Mesa:
//!
//! ```text
//! code = 10_000_000 + round((value + 2.0) * 1000) * 1000 + index
//! ```
//!
//! giving 0.001 resolution over -2.0..2.0 (both panels' sliders step by 0.01)
//! and indices 1..=999 (0 = explicit no-op).
//!
//! The 10^7 tag is not decoration. Without it, the *untouched* default value
//! of `fps` decodes as a command: projectM 4.1.6 starts every instance at 35,
//! and `35 % 1000` is a perfectly valid index, so a freshly patched preset
//! silently latched -2.0 into slot 35 before the host had made a single call.
//! The preset-side prologue therefore ignores anything at or below 9_999_999,
//! which covers every frame rate a projectM instance could plausibly hold.
//!
//! Max code is 14_000_999. The Milkdrop expression evaluator was measured to
//! be **32-bit float** (feeding it 2_000_000_123 reads back as 2_000_000_128,
//! the f32-rounded value), so the real ceiling for exact integers is 2^24 =
//! 16_777_216. The format fits with 16% to spare, and there is no room to pack
//! a second parameter into the same word.
//!
//! # The collision, and why the `fps` rewrite is mandatory
//!
//! 2871 of the 9795 presets in the reference library (29%) read `fps`
//! themselves, almost always for framerate-independent physics (`60/fps`,
//! `.../fps`). Feeding them a code word instead would silently collapse their
//! motion. `patch_preset` therefore also rewrites every standalone `fps`
//! identifier in the preset's own equation blocks to a literal the caller
//! supplies.
//!
//! That literal is a caller decision on purpose. `parameters.h` claims the
//! default is 60; on this libprojectM it is measured to be
//! [`MEASURED_DEFAULT_FPS`] (35), never 60, so hardcoding 60 would have turned
//! `60/fps` from 1.714 into 1.0 and sped 29% of the corpus up by 42%. Pass
//! [`MEASURED_DEFAULT_FPS`] to preserve exactly what presets see today, or the
//! deck's real target frame rate (the app already has one: `ui::quality`'s
//! 30/45/60 setting) to give them a physically honest value.

/// Index range of the packed word. Indices are `1..=MAX_INDEX`; 0 is a no-op
/// the preset ignores, so the host can write "nothing changed this frame".
const INDEX_SPAN: i32 = 1000;

/// Fixed-point scale of the value half of the packed word: 0.001 resolution.
const VALUE_SCALE: f64 = 1000.0;

/// Added before scaling so negative values fit an unsigned code, subtracted
/// again by the preset-side decoder.
const VALUE_OFFSET: f64 = 2.0;

/// Tags every encoded word so a raw frame rate sitting in `fps` cannot be
/// mistaken for one. See the module docs: without it, projectM's own start-up
/// value of 35 decodes as a write to slot 35.
const WORD_TAG: i32 = 10_000_000;

/// Preset-side threshold: `od_c` at or below this is a raw frame rate, not a
/// command, and the demux must ignore it.
const WORD_GUARD: i32 = WORD_TAG - 1;

/// Lowest index a [`PatchTarget`] may claim. 0 is reserved: it is what the
/// preset-side guard produces when the word is not a command, so a target
/// sitting there would latch garbage on every non-command frame.
pub const MIN_INDEX: u16 = 1;

/// Highest usable parameter index.
pub const MAX_INDEX: u16 = (INDEX_SPAN - 1) as u16;

/// Lowest value the side channel can carry.
pub const VALUE_MIN: f64 = -VALUE_OFFSET;

/// Highest value the side channel can carry.
pub const VALUE_MAX: f64 = VALUE_OFFSET;

/// Opens the first appended line of an equation block whose own lines already
/// compile to a non-empty program.
///
/// libprojectM concatenates a block's `per_frame_1..N` lines into a single
/// Milkdrop program, and 703 of the 9795 presets in the reference library
/// (7.2%, and 1162 = 11.9% for `per_frame_init`) end their last statement
/// **without** a `;`. That is legal as the final statement of the original
/// program, and a syntax error the moment anything is appended after it,
/// which takes down the *whole* block, not just the appended lines: measured
/// against real libprojectM, a preset ending in `q1 = 0.5` stopped evaluating
/// its own equations entirely once one line was appended.
///
/// A `;` in the *middle* of a program is an inert empty statement, so it is
/// emitted unconditionally rather than sniffed for. At the very *start* it is
/// not: a program beginning with `;` fails to compile, also measured.
///
/// "Start of the program" is **not** the same as "the block has no numbered
/// lines": the condition that matters is whether the preset's own lines
/// contribute at least one real statement (see [`is_statement`]). 25 of the
/// 9795 reference presets have a block whose every line is a comment
/// (`per_frame_init_1=//decay = 0.94`); those compile to an empty program, so
/// an appended `;` lands first and projectM rejects the whole preset. Gating
/// on line *count* instead of statement content made all 25 fail to load,
/// verified against real libprojectM, and silently, since a rejected load
/// leaves the deck on its previous preset.
const SEPARATOR: &str = ";";

/// What `projectm_get_fps` actually returns on a fresh libprojectM 4.1.6
/// instance, measured rather than read off the header (which documents 60 and
/// is wrong). Pass this to [`patch_preset`] to leave the corpus's
/// framerate-dependent physics exactly as it behaves today.
pub const MEASURED_DEFAULT_FPS: i32 = 35;

/// Packs one `(index, value)` pair into the word `Deck::set_param` writes
/// through `projectm_set_fps`. Index is clamped to `MAX_INDEX` (0 stays 0, the
/// explicit no-op), value to `VALUE_MIN..=VALUE_MAX`.
pub fn encode_param(index: u16, value: f64) -> i32 {
    let index = i32::from(index.min(MAX_INDEX));
    let scaled = ((value.clamp(VALUE_MIN, VALUE_MAX) + VALUE_OFFSET) * VALUE_SCALE).round() as i32;
    WORD_TAG + scaled * INDEX_SPAN + index
}

/// How a latched value reaches the Milkdrop variable it drives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Apply {
    /// `var = var * od_pN;` scales whatever the preset computed (Time's
    /// rot/warp/dx/dy/wave_a multipliers).
    Multiply(String),
    /// `var = 1 + (var - 1) * od_pN;` scales the variable's *departure from
    /// 1*, leaving 1 fixed. Required for the Milkdrop variables whose
    /// neutral value is 1 rather than 0 (`zoom`, `sx`, `sy`): a plain
    /// [`Apply::Multiply`] on those turns a multiplier of 0 into a collapsed
    /// image and a multiplier of 2 on a typical `zoom = 1.01` into `2.02`, a
    /// runaway. Mirrors the web app's own Time semantics
    /// (`core::time_params::inject_time_params`'s `a.zoom = 1 + (a.zoom - 1)
    /// * zoomMult` lines), which this exists to reproduce.
    ScaleAroundOne(String),
    /// `var = od_pN;` replaces it outright (Qvar's `q1`..`q32` overrides).
    Assign(String),
}

/// One host-driven parameter: which slot of the side channel carries it, the
/// value to bake in as the preset's starting point, and the Milkdrop variable
/// it drives.
#[derive(Debug, Clone, PartialEq)]
pub struct PatchTarget {
    /// Clamped to `MIN_INDEX..=MAX_INDEX` by [`patch_preset`]; index 0 is
    /// reserved for the no-op word and must never carry a target.
    pub index: u16,
    pub initial: f64,
    pub apply: Apply,
}

/// Rewrites `text` so a preset loaded from it reads this host's side channel.
///
/// Two transformations, both purely lexical:
///
/// 1. every standalone `fps` identifier in the preset's own equation blocks
///    (`per_frame_init_N`, `per_frame_N`, `per_pixel_N`, `shape_N_per_frameM`,
///    `wave_N_per_frameM`, `wave_N_per_pointM`) becomes `substituted_fps`;
/// 2. a demux prologue, one latch per distinct target index, and one
///    application line per target are appended after the preset's own
///    equations, numbered past the highest existing index so they run last.
///
/// Appending is not a detail: the application lines have to run *after* the
/// preset has computed `zoom`/`rot`/… to be able to scale them, and
/// libprojectM reads `per_frame_N` from 1 upwards and stops at the first
/// missing index (measured), so appending past `max(N)` is the only
/// placement that neither renumbers the preset's own equations nor lands in
/// dead space. The corollary is that a target can only drive a variable
/// projectM reads back *out* of the per-frame block; `time` in particular is
/// a per-frame input that is re-set by the engine every frame (measured: a
/// value written to it does not survive to the next frame), so it cannot be
/// driven from here at all.
///
/// `substituted_fps` is the caller's call: [`MEASURED_DEFAULT_FPS`] preserves
/// today's behaviour exactly, the deck's real target frame rate is more
/// physically honest. See the module docs for why there is no safe default.
///
/// Shader blocks (`warp_N` / `comp_N`) are **not** rewritten, and that is a
/// known accepted risk rather than a non-issue: once any target on this deck
/// is live, those blocks read the raw code word (order 10^7) as their `fps`.
/// The 10 lines across the whole 9795-preset reference library that do this
/// will render wrong. Building an HLSL rewriter for 0.1% of the corpus was
/// judged not worth it.
pub fn patch_preset(text: &str, targets: &[PatchTarget], substituted_fps: i32) -> String {
    let mut out = String::with_capacity(text.len() + 256 + targets.len() * 96);
    let mut max_frame = 0u32;
    let mut max_init = 0u32;
    // Whether each block contributes at least one real statement to the
    // compiled program: NOT merely whether it has numbered lines. See
    // `SEPARATOR`: a block whose every line is a comment compiles to an empty
    // program, and a program that *starts* with `;` does not compile.
    let mut frame_has_statement = false;
    let mut init_has_statement = false;

    for line in text.lines() {
        match split_key(line) {
            Some((key, value)) => {
                if let Some(n) = equation_index(key, "per_frame_init_") {
                    max_init = max_init.max(n);
                    init_has_statement |= is_statement(value);
                } else if let Some(n) = equation_index(key, "per_frame_") {
                    max_frame = max_frame.max(n);
                    frame_has_statement |= is_statement(value);
                }
                if is_equation_block(key) {
                    out.push_str(key);
                    out.push('=');
                    out.push_str(&substitute_fps(value, substituted_fps));
                } else {
                    out.push_str(line);
                }
            }
            None => out.push_str(line),
        }
        out.push('\n');
    }

    if targets.is_empty() {
        return out;
    }

    let slot = |t: &PatchTarget| t.index.clamp(MIN_INDEX, MAX_INDEX);

    // One register per *index*, not per target: two targets may deliberately
    // share a slot when one host parameter drives two Milkdrop variables
    // (Time's Stretch drives both `sx` and `sy`), and emitting the seed or
    // the latch twice for the same register is redundant noise in every
    // patched preset.
    let mut seeded: Vec<u16> = Vec::with_capacity(targets.len());
    let mut n_init = max_init;
    for target in targets {
        let i = slot(target);
        if seeded.contains(&i) {
            continue;
        }
        n_init += 1;
        // `SEPARATOR` on the first appended line only, and only when this
        // block already compiles to a non-empty program. See its own docs.
        let sep = if seeded.is_empty() && init_has_statement { SEPARATOR } else { "" };
        seeded.push(i);
        out.push_str(&format!(
            "per_frame_init_{n_init}={sep}od_p{i} = {v};\n",
            v = fmt_num(target.initial)
        ));
    }

    let mut n = max_frame;
    let mut emit = |code: String| {
        n += 1;
        let sep = if n == max_frame + 1 && frame_has_statement { SEPARATOR } else { "" };
        out.push_str(&format!("per_frame_{n}={sep}{code}\n"));
    };
    emit("od_c = fps;".to_string());
    // Guard first: anything at or below WORD_GUARD is a real frame rate (or
    // projectM's own start-up value), not a command, and collapses od_i to the
    // reserved 0 so no latch fires.
    emit(format!("od_g = above(od_c,{WORD_GUARD});"));
    emit(format!("od_i = od_g * (od_c % {INDEX_SPAN});"));
    emit(format!(
        "od_v = int((od_c - {WORD_TAG})/{INDEX_SPAN})/{scale} - {offset};",
        scale = fmt_num(VALUE_SCALE),
        offset = fmt_num(VALUE_OFFSET)
    ));
    for &i in &seeded {
        emit(format!(
            "od_p{i} = equal(od_i,{i})*od_v + (1-equal(od_i,{i}))*od_p{i};"
        ));
    }
    for target in targets {
        let i = slot(target);
        emit(match &target.apply {
            Apply::Multiply(var) => format!("{var} = {var} * od_p{i};"),
            Apply::ScaleAroundOne(var) => format!("{var} = 1 + ({var} - 1) * od_p{i};"),
            Apply::Assign(var) => format!("{var} = od_p{i};"),
        });
    }
    out
}

/// Whether an equation line contributes anything to the compiled program:
/// the code before any `//` comment, trimmed, is non-empty. A line that is
/// only a comment (`per_frame_init_1=//decay = 0.94`) contributes nothing, so
/// a block made entirely of such lines compiles to an *empty* program, which
/// is exactly the case where [`SEPARATOR`] must not be emitted.
fn is_statement(value: &str) -> bool {
    let code = match value.find("//") {
        Some(i) => &value[..i],
        None => value,
    };
    !code.trim().is_empty()
}

/// Splits `key=value`, or returns `None` for blank lines and `[section]`
/// headers.
fn split_key(line: &str) -> Option<(&str, &str)> {
    let eq = line.find('=')?;
    let key = &line[..eq];
    if key.is_empty() || key.starts_with('[') {
        return None;
    }
    Some((key, &line[eq + 1..]))
}

/// True for the preset's own equation blocks, the ones that see the Milkdrop
/// `fps` variable. Shader blocks (`warp_`/`comp_`) are excluded on purpose.
fn is_equation_block(key: &str) -> bool {
    !key.starts_with("warp_")
        && !key.starts_with("comp_")
        && (key.contains("per_frame") || key.contains("per_pixel") || key.contains("per_point"))
}

/// `equation_index("per_frame_12", "per_frame_")` → `Some(12)`. Returns `None`
/// when the remainder is not a bare number, so `per_frame_init_3` is not
/// mistaken for a `per_frame_` line.
fn equation_index(key: &str, prefix: &str) -> Option<u32> {
    key.strip_prefix(prefix)?.parse().ok()
}

/// Replaces standalone `fps` identifiers with `literal`, leaving `myfps`,
/// `fps2` and `bass_fps` alone.
fn substitute_fps(code: &str, literal: i32) -> String {
    let bytes = code.as_bytes();
    let mut out = String::with_capacity(code.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i..].starts_with(b"fps")
            && !i.checked_sub(1).is_some_and(|p| is_ident_byte(bytes[p]))
            && !bytes.get(i + 3).copied().is_some_and(is_ident_byte)
        {
            out.push_str(&literal.to_string());
            i += 3;
        } else {
            // Advance a whole char so multi-byte UTF-8 in comments survives.
            let step = code[i..].chars().next().map_or(1, char::len_utf8);
            out.push_str(&code[i..i + step]);
            i += step;
        }
    }
    out
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Milkdrop equations have no notion of Rust's `1e-5`/`inf` spellings, so
/// render numbers plainly and drop a trailing `.0`.
fn fmt_num(v: f64) -> String {
    let s = format!("{v:.5}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-" {
        "0".to_string()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assign(index: u16, var: &str) -> PatchTarget {
        PatchTarget {
            index,
            initial: 0.0,
            apply: Apply::Assign(var.to_string()),
        }
    }

    // ---- encode_param ----

    #[test]
    fn encodes_index_in_the_low_digits_and_value_in_the_high_ones() {
        // 0.7 → (0.7 + 2) * 1000 = 2700, index 1, plus the 10^7 tag.
        assert_eq!(encode_param(1, 0.7), 12_700_001);
    }

    #[test]
    fn encodes_the_range_endpoints_without_overflowing() {
        assert_eq!(encode_param(1, VALUE_MIN), 10_000_001);
        assert_eq!(encode_param(MAX_INDEX, VALUE_MAX), 14_000_999);
        // The Milkdrop evaluator was measured to be 32-bit float, so exactness
        // really does stop at 2^24. This is the binding constraint, not int32.
        assert!(encode_param(MAX_INDEX, VALUE_MAX) < 1 << 24);
    }

    #[test]
    fn encodes_zero_index_as_the_no_op_slot() {
        assert_eq!(encode_param(0, 0.0) % INDEX_SPAN, 0);
    }

    #[test]
    fn clamps_out_of_range_values_and_indices() {
        assert_eq!(encode_param(1, 99.0), encode_param(1, VALUE_MAX));
        assert_eq!(encode_param(1, -99.0), encode_param(1, VALUE_MIN));
        assert_eq!(encode_param(60_000, 0.0), encode_param(MAX_INDEX, 0.0));
    }

    #[test]
    fn round_trips_every_representable_step() {
        let mut v = VALUE_MIN;
        while v <= VALUE_MAX {
            let code = encode_param(7, v);
            assert_eq!(code % INDEX_SPAN, 7);
            let decoded = f64::from((code - WORD_TAG) / INDEX_SPAN) / VALUE_SCALE - VALUE_OFFSET;
            assert!((decoded - v).abs() < 1e-9, "{v} decoded as {decoded}");
            v += 0.01;
        }
    }

    #[test]
    fn tags_every_word_clear_of_any_plausible_raw_frame_rate() {
        // The whole point of the tag: nothing a projectM instance could be
        // holding in `fps` may pass the preset-side guard. 35 is what 4.1.6
        // actually starts at; 30/45/60 are the app's own quality settings.
        for raw in [0, 1, MEASURED_DEFAULT_FPS, 30, 45, 60, 240, 1000, WORD_GUARD] {
            assert!(raw <= WORD_GUARD, "{raw} would be read as a command");
        }
        let mut v = VALUE_MIN;
        while v <= VALUE_MAX {
            for index in [0, 1, 40, MAX_INDEX] {
                assert!(encode_param(index, v) > WORD_GUARD);
            }
            v += 0.01;
        }
    }

    // ---- fps substitution ----

    #[test]
    fn substitutes_the_fps_variable_in_every_equation_block() {
        let src = "per_frame_1=vy = vy - 0.0001*60/fps;\n\
                   per_pixel_1=zoom = zoom + fps*0.001;\n\
                   per_frame_init_1=k = fps;\n\
                   shape_0_per_frame1=r = fps/120;\n\
                   wave_0_per_frame1=a = fps;\n\
                   wave_0_per_point1=x = x + fps;\n";
        let out = patch_preset(src, &[], MEASURED_DEFAULT_FPS);
        assert!(!out.contains("fps"), "{out}");
        assert_eq!(out.matches("35").count(), 6, "{out}");
        // The literal 60 already in the source is left alone.
        assert!(out.contains("vy - 0.0001*60/35;"), "{out}");
    }

    #[test]
    fn substitutes_whatever_literal_the_caller_supplies() {
        // The caller owns this number: 35 preserves today's behaviour, the
        // deck's real rate is more honest. Hardcoding 60 (which the header
        // wrongly documents as the default) would have sped 29% of the corpus
        // up by 42%.
        let src = "per_frame_1=vy = vy - 0.0001*60/fps;\n";
        assert!(patch_preset(src, &[], 45).contains("60/45;"));
        assert!(patch_preset(src, &[], 144).contains("60/144;"));
    }

    #[test]
    fn leaves_shader_blocks_alone() {
        let src = "warp_1=`   ret = float3(fps,0,0);\ncomp_2=`   ret *= fps;\n";
        assert_eq!(patch_preset(src, &[], MEASURED_DEFAULT_FPS), src);
    }

    #[test]
    fn leaves_identifiers_that_merely_contain_fps_alone() {
        let src = "per_frame_1=myfps = fps2 + _fps + bass_fps + fps;\n";
        let out = patch_preset(src, &[], MEASURED_DEFAULT_FPS);
        assert!(out.contains("myfps = fps2 + _fps + bass_fps + 35;"), "{out}");
    }

    #[test]
    fn leaves_non_equation_keys_and_section_headers_alone() {
        let src = "[preset00]\nfRating=5.000\nnWaveMode=4\n";
        assert_eq!(patch_preset(src, &[], MEASURED_DEFAULT_FPS), src);
    }

    // ---- appended demux / latch / application ----

    #[test]
    fn appends_the_demux_prologue_after_the_presets_own_equations() {
        let out = patch_preset("per_frame_1=zoom = 1.01;\n", &[assign(1, "q1")], MEASURED_DEFAULT_FPS);
        let own = out.find("zoom = 1.01;").unwrap();
        let demux = out.find("od_c = fps;").unwrap();
        assert!(own < demux, "{out}");
        assert!(out.contains("per_frame_2=;od_c = fps;"), "{out}");
        assert!(out.contains("per_frame_3=od_g = above(od_c,9999999);"), "{out}");
        assert!(out.contains("per_frame_4=od_i = od_g * (od_c % 1000);"), "{out}");
        assert!(
            out.contains("per_frame_5=od_v = int((od_c - 10000000)/1000)/1000 - 2;"),
            "{out}"
        );
    }

    #[test]
    fn gates_the_index_on_the_guard_so_a_raw_frame_rate_latches_nothing() {
        // The C2 regression. A freshly patched preset with zero set_param
        // calls sits at projectM's raw default (35), which without the guard
        // decoded as "write -2.0 to slot 35". The guard collapses od_i to the
        // reserved 0, and every latch line tests `equal(od_i, N)` for N >= 1,
        // so nothing fires and every slot keeps its baked-in `initial`.
        let targets = [PatchTarget {
            index: MEASURED_DEFAULT_FPS as u16,
            initial: 0.5,
            apply: Apply::Assign("q1".to_string()),
        }];
        let out = patch_preset("", &targets, MEASURED_DEFAULT_FPS);
        assert!(out.contains("od_g = above(od_c,9999999);"), "{out}");
        assert!(out.contains("od_i = od_g * (od_c % 1000);"), "{out}");
        assert!(out.contains("per_frame_init_1=od_p35 = 0.5;"), "{out}");
        // No latch line may test against the reserved index 0.
        assert!(!out.contains("equal(od_i,0)"), "{out}");
    }

    #[test]
    fn never_lets_a_target_claim_the_reserved_index_zero() {
        let targets = [PatchTarget {
            index: 0,
            initial: 1.0,
            apply: Apply::Multiply("zoom".to_string()),
        }];
        let out = patch_preset("", &targets, MEASURED_DEFAULT_FPS);
        assert!(!out.contains("od_p0"), "{out}");
        assert!(out.contains("zoom = zoom * od_p1;"), "{out}");
    }

    #[test]
    fn numbers_appended_lines_past_the_highest_existing_index() {
        // Out-of-order numbering, and a per_frame_init block that
        // outnumbers per_frame, must both land past the right maximum.
        let src = "per_frame_9=a = 1;\nper_frame_2=b = 2;\nper_frame_init_4=c = 3;\n";
        let out = patch_preset(src, &[assign(1, "q1")], MEASURED_DEFAULT_FPS);
        assert!(out.contains("per_frame_init_5=;od_p1 = 0;"), "{out}");
        assert!(out.contains("per_frame_10=;od_c = fps;"), "{out}");
    }

    #[test]
    fn does_not_mistake_per_frame_init_for_a_per_frame_line() {
        let out = patch_preset("per_frame_init_50=a = 1;\n", &[assign(1, "q1")], MEASURED_DEFAULT_FPS);
        assert!(out.contains("per_frame_1=od_c = fps;"), "{out}");
        assert!(out.contains("per_frame_init_51=;od_p1 = 0;"), "{out}");
    }

    #[test]
    fn latches_each_target_against_its_own_index() {
        let out = patch_preset("", &[assign(3, "q3"), assign(40, "q7")], MEASURED_DEFAULT_FPS);
        assert!(
            out.contains("od_p3 = equal(od_i,3)*od_v + (1-equal(od_i,3))*od_p3;"),
            "{out}"
        );
        assert!(
            out.contains("od_p40 = equal(od_i,40)*od_v + (1-equal(od_i,40))*od_p40;"),
            "{out}"
        );
    }

    #[test]
    fn emits_multiply_and_assign_application_lines() {
        let targets = [
            PatchTarget {
                index: 1,
                initial: 1.0,
                apply: Apply::Multiply("zoom".to_string()),
            },
            assign(2, "q1"),
        ];
        let out = patch_preset("", &targets, MEASURED_DEFAULT_FPS);
        assert!(out.contains("zoom = zoom * od_p1;"), "{out}");
        assert!(out.contains("q1 = od_p2;"), "{out}");
    }

    #[test]
    fn emits_the_scale_around_one_application_line() {
        // `zoom`/`sx`/`sy` are neutral at 1, not 0: a plain multiply would
        // make a 0 multiplier collapse the image and a 2 multiplier turn a
        // typical `zoom = 1.01` into 2.02.
        let targets = [PatchTarget {
            index: 2,
            initial: 1.0,
            apply: Apply::ScaleAroundOne("zoom".to_string()),
        }];
        let out = patch_preset("", &targets, MEASURED_DEFAULT_FPS);
        assert!(out.contains("zoom = 1 + (zoom - 1) * od_p2;"), "{out}");
    }

    #[test]
    fn seeds_and_latches_a_shared_index_exactly_once() {
        // Time's Stretch drives both `sx` and `sy` from one slider, so two
        // targets deliberately share a register. Two applications, but only
        // one seed and one latch.
        let targets = [
            PatchTarget { index: 7, initial: 1.0, apply: Apply::ScaleAroundOne("sx".to_string()) },
            PatchTarget { index: 7, initial: 1.0, apply: Apply::ScaleAroundOne("sy".to_string()) },
        ];
        let out = patch_preset("", &targets, MEASURED_DEFAULT_FPS);
        assert_eq!(out.matches("od_p7 = 1;").count(), 1, "{out}");
        assert_eq!(out.matches("od_p7 = equal(od_i,7)").count(), 1, "{out}");
        assert!(out.contains("sx = 1 + (sx - 1) * od_p7;"), "{out}");
        assert!(out.contains("sy = 1 + (sy - 1) * od_p7;"), "{out}");
    }

    #[test]
    fn keeps_appended_line_numbering_contiguous_with_a_shared_index() {
        // libprojectM stops reading `per_frame_N` at the first gap
        // (measured), so deduplicating the latch lines must not leave a hole
        // in the numbering of what follows.
        let targets = [
            PatchTarget { index: 7, initial: 1.0, apply: Apply::ScaleAroundOne("sx".to_string()) },
            PatchTarget { index: 7, initial: 1.0, apply: Apply::ScaleAroundOne("sy".to_string()) },
            PatchTarget { index: 8, initial: 1.0, apply: Apply::Multiply("wave_a".to_string()) },
        ];
        let out = patch_preset("per_frame_1=zoom = 1.01;\n", &targets, MEASURED_DEFAULT_FPS);
        assert_contiguous_equation_numbering(&out);
    }

    #[test]
    fn keeps_appended_line_numbering_contiguous_for_plain_targets() {
        let targets = [
            PatchTarget { index: 2, initial: 1.0, apply: Apply::ScaleAroundOne("zoom".to_string()) },
            PatchTarget { index: 3, initial: 1.0, apply: Apply::Multiply("rot".to_string()) },
        ];
        let out = patch_preset(
            "per_frame_init_1=a = 0;\nper_frame_1=zoom = 1.01;\nper_frame_2=rot = 0.1;\n",
            &targets,
            MEASURED_DEFAULT_FPS,
        );
        assert_contiguous_equation_numbering(&out);
    }

    /// Asserts `per_frame_N` and `per_frame_init_N` both run 1..len with no
    /// hole: libprojectM stops at the first missing index, so a gap
    /// silently kills every line after it.
    fn assert_contiguous_equation_numbering(out: &str) {
        for prefix in ["per_frame_", "per_frame_init_"] {
            let mut seen: Vec<u32> = out
                .lines()
                .filter_map(|l| l.split('=').next())
                .filter_map(|k| equation_index(k, prefix))
                .collect();
            seen.sort_unstable();
            seen.dedup();
            let expected: Vec<u32> = (1..=seen.len() as u32).collect();
            assert_eq!(seen, expected, "{prefix} numbering has a gap:\n{out}");
        }
    }

    #[test]
    fn applies_every_target_after_every_latch() {
        let targets = [
            PatchTarget {
                index: 1,
                initial: 1.0,
                apply: Apply::Multiply("zoom".to_string()),
            },
            assign(2, "q1"),
        ];
        let out = patch_preset("", &targets, MEASURED_DEFAULT_FPS);
        let last_latch = out.rfind("od_p2 = equal").unwrap();
        assert!(out.find("zoom = zoom * od_p1;").unwrap() > last_latch, "{out}");
    }

    #[test]
    fn bakes_the_initial_value_into_per_frame_init() {
        let targets = [PatchTarget {
            index: 1,
            initial: 1.0,
            apply: Apply::Multiply("zoom".to_string()),
        }];
        // Neutral until the first side-channel write lands, so loading a preset
        // mid-set does not flash the preset's unscaled look for one frame.
        assert!(patch_preset("", &targets, MEASURED_DEFAULT_FPS).contains("od_p1 = 1;"));
    }

    #[test]
    fn renders_fractional_initials_without_rust_float_spellings() {
        let targets = [assign(1, "q1")];
        let mut targets = targets.to_vec();
        targets[0].initial = -1.755;
        let out = patch_preset("", &targets, MEASURED_DEFAULT_FPS);
        assert!(out.contains("od_p1 = -1.755;"), "{out}");
    }

    #[test]
    fn opens_each_appended_block_with_a_statement_separator() {
        // 703 of the 9795 reference presets (7.2%) end their per_frame
        // program without a `;`, and 1162 (11.9%) end per_frame_init that
        // way. libprojectM concatenates a block's lines into one program, so
        // appending after an unterminated statement is a syntax error that
        // kills the *whole* block. Measured: a preset ending in `q1 = 0.5`
        // stopped evaluating its own equations entirely once one line was
        // appended. Exactly one leading `;` per appended block; a second one
        // on the following lines would be noise.
        let out = patch_preset(
            "per_frame_init_1=a = 1\nper_frame_1=zoom = 1.01\n",
            &[assign(1, "q1"), assign(2, "q2")],
            MEASURED_DEFAULT_FPS,
        );
        assert!(out.contains("per_frame_init_2=;od_p1 = 0;"), "{out}");
        assert!(out.contains("per_frame_init_3=od_p2 = 0;"), "{out}");
        assert!(out.contains("per_frame_2=;od_c = fps;"), "{out}");
        assert!(out.contains("per_frame_3=od_g = "), "{out}");
        assert_eq!(out.matches("=;").count(), 2, "{out}");
    }

    #[test]
    fn treats_an_all_comment_block_as_having_no_statement() {
        // 25 of the 9795 reference presets have a block whose every line is a
        // comment. Those compile to an *empty* program, so an appended `;`
        // would land first and projectM rejects the whole preset, verified
        // against real libprojectM, and silently, since a rejected load leaves
        // the deck on its previous preset. Gating on "has numbered lines"
        // instead of "has a statement" is what broke them.
        let out = patch_preset(
            "per_frame_init_1=//decay = 0.94\nper_frame_1=zoom = 1.01;\n",
            &[assign(1, "q1")],
            MEASURED_DEFAULT_FPS,
        );
        assert!(out.contains("per_frame_init_2=od_p1 = 0;"), "{out}");
        // The per_frame block does have a statement, so it still gets one.
        assert!(out.contains("per_frame_2=;od_c = fps;"), "{out}");
        assert_eq!(out.matches("=;").count(), 1, "{out}");
    }

    #[test]
    fn treats_an_all_comment_per_frame_block_as_having_no_statement() {
        let out = patch_preset(
            "per_frame_init_1=k = 1;\nper_frame_1=  // nothing to see\nper_frame_2=\t//still nothing\n",
            &[assign(1, "q1")],
            MEASURED_DEFAULT_FPS,
        );
        assert!(out.contains("per_frame_3=od_c = fps;"), "{out}");
        // The per_frame_init block does have a statement, so it still gets one.
        assert!(out.contains("per_frame_init_2=;od_p1 = 0;"), "{out}");
        assert_eq!(out.matches("=;").count(), 1, "{out}");
    }

    #[test]
    fn a_single_target_is_enough_to_trigger_the_all_comment_case() {
        // Not a line-count effect: one target already appends a first line to
        // each block, which is all it takes.
        let out = patch_preset("per_frame_init_1=// only a comment\n", &[assign(1, "q1")], MEASURED_DEFAULT_FPS);
        assert!(!out.contains("=;"), "{out}");
    }

    #[test]
    fn counts_code_before_a_trailing_comment_as_a_statement() {
        // The inverse mistake: treating any line containing `//` as a comment
        // line would drop the separator where it is genuinely required.
        let out = patch_preset("per_frame_1=zoom = 1.01 // scale it\n", &[assign(1, "q1")], MEASURED_DEFAULT_FPS);
        assert!(out.contains("per_frame_2=;od_c = fps;"), "{out}");
    }

    #[test]
    fn never_starts_a_block_with_the_separator() {
        // A Milkdrop program that *begins* with `;` fails to compile
        // (measured). The separator is only ever a separator. 121 presets
        // in the library have no per_frame block at all, and every preset
        // that lacks `per_frame_init` hits the same case for that block.
        let out = patch_preset("fRating=5.000\n", &[assign(1, "q1")], MEASURED_DEFAULT_FPS);
        assert!(out.contains("per_frame_init_1=od_p1 = 0;"), "{out}");
        assert!(out.contains("per_frame_1=od_c = fps;"), "{out}");
        assert!(!out.contains("=;"), "{out}");

        // A preset with per_frame but no per_frame_init: separator on one
        // block, not the other.
        let out = patch_preset("per_frame_1=zoom = 1.01\n", &[assign(1, "q1")], MEASURED_DEFAULT_FPS);
        assert!(out.contains("per_frame_init_1=od_p1 = 0;"), "{out}");
        assert!(out.contains("per_frame_2=;od_c = fps;"), "{out}");
        assert_eq!(out.matches("=;").count(), 1, "{out}");
    }

    #[test]
    fn appends_nothing_when_there_are_no_targets() {
        let out = patch_preset("per_frame_1=zoom = 1.01;\n", &[], MEASURED_DEFAULT_FPS);
        assert!(!out.contains("od_c"), "{out}");
    }

    #[test]
    fn handles_crlf_input_normalising_it_to_lf() {
        // Plenty of .milk files in the wild are CRLF. The patched text only
        // ever goes straight to projectm_load_preset_data, never back to disk,
        // so normalising is fine; parsing it correctly is what matters.
        let out = patch_preset("fRating=5.000\r\nper_frame_1=a = fps;\r\n", &[], MEASURED_DEFAULT_FPS);
        assert_eq!(out, "fRating=5.000\nper_frame_1=a = 35;\n");
    }
}
