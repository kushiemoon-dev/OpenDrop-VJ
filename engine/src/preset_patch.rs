//! Host → running-preset numeric injection for projectM 4.
//!
//! # Why this exists
//!
//! Time (8 params/deck) and Qvar (32 watches/deck) both need the host to feed
//! a number into a preset that is already rendering, continuously: a slider
//! dragged live, or the LFO engine ticking every frame. libprojectM 4.1.6
//! exports 47 C functions and not one of them addresses a preset-internal
//! variable.
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
mod tests;
