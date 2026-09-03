//! Port of OpenDrop-VJ `src/lib/engine/commands.ts`: the `CommandRegistry` keystone.
//!
//! 223 `CommandId` variants, `kind: Trigger | Range`, contract `run(value01, ctx)`
//! preserved identically. 221 of the 223 carry a real setter; only
//! `CommandId::LfoRateUp`/`LfoRateDown` are still `noop` stubs, permanently
//! and by design: see `CommandRegistry`'s doc comment for why.

use std::collections::HashMap;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Deck {
    #[default]
    A,
    B,
}

pub trait CommandContext {
    fn get_crossfader(&self) -> f64;
    fn set_crossfader(&mut self, v: f64);
    fn get_active_deck(&self) -> Deck;
    fn switch_active_deck(&mut self);
    fn navigate_preset(&mut self, deck: Deck, direction: i32);
    fn toggle_playlist(&mut self, deck: Deck);
    fn playlist_next(&mut self, deck: Deck);
    fn playlist_prev(&mut self, deck: Deck);
    fn get_playlist_playing(&self, deck: Deck) -> bool;
    fn advance_overlay_queue(&mut self, direction: i32);
    fn set_color_hue_a(&mut self, v: f64);
    fn set_color_sat_a(&mut self, v: f64);
    fn set_color_bright_a(&mut self, v: f64);
    fn set_color_contrast_a(&mut self, v: f64);
    fn set_color_invert_a(&mut self, v: f64);
    fn set_color_hue_b(&mut self, v: f64);
    fn set_color_sat_b(&mut self, v: f64);
    fn set_color_bright_b(&mut self, v: f64);
    fn set_color_contrast_b(&mut self, v: f64);
    fn set_color_invert_b(&mut self, v: f64);
    fn set_composite_blend(&mut self, slot: usize, v: f64);
    fn set_composite_luma_black(&mut self, slot: usize, v: f64);
    fn set_composite_luma_white(&mut self, slot: usize, v: f64);
    fn set_composite_color_hue(&mut self, slot: usize, v: f64);
    fn set_composite_color_tol(&mut self, slot: usize, v: f64);
    /// Starts a snapshot recall for `slot` (0..8): captures the current
    /// value of every `CommandId` the target slot's snapshot holds, then
    /// arms `Show::active_recall` so the per-frame loop in
    /// `app::about_to_wait` can interpolate toward it. A single method
    /// parameterized by slot, not 8 separate methods: same shape as
    /// `set_composite_blend(slot, v)` above.
    fn recall_snapshot(&mut self, slot: usize);
    /// Toggles timeline playback (Step 5). Starting playback resets the
    /// loop's internal progress to 0 rather than resuming stale progress
    /// or jumping in time: see `Show::tick_timeline`'s doc comment for why
    /// that's a dt-accumulated elapsed counter rather than an absolute
    /// wall-clock timestamp. No parameter, same shape as
    /// `switch_active_deck` above.
    fn toggle_timeline(&mut self);
    /// Toggles the strobe on/off (Step 10 of the Phase 8 VJ-panels plan).
    /// No parameter, same shape as `toggle_timeline`/`switch_active_deck`
    /// above: rate/intensity/color have no `CommandContext` setter, only
    /// the toggle is in the transversal keyboard/MIDI/OSC/remote-ws list.
    fn toggle_strobe(&mut self);
    /// The 8 per-deck Time multipliers (Step 8). One method per multiplier,
    /// each parameterized by deck slot (0..4): same shape as
    /// `set_composite_blend(slot, v)` above, not 32 separate methods for the
    /// 32 `CommandId::Time*` commands. `v` is in the panel's own 0..
    /// [`crate::time_params::TIME_MULT_MAX`] range, not 0..1: the registry
    /// entries convert with [`time_mult`] first.
    fn set_time_speed(&mut self, slot: usize, v: f64);
    fn set_time_zoom(&mut self, slot: usize, v: f64);
    fn set_time_rot(&mut self, slot: usize, v: f64);
    fn set_time_warp(&mut self, slot: usize, v: f64);
    fn set_time_dx(&mut self, slot: usize, v: f64);
    fn set_time_dy(&mut self, slot: usize, v: f64);
    fn set_time_stretch(&mut self, slot: usize, v: f64);
    fn set_time_wave(&mut self, slot: usize, v: f64);
    /// One q-var override (Step 9), for the 128 `CommandId::Qvar{n}_{slot}`
    /// commands: `n` is the 1-indexed q-var (1..=32, matching
    /// `q_vars::with_q_var_value`'s own convention), `slot` the deck (0..4).
    /// A single method rather than Time's one-per-multiplier shape because
    /// the 32 q-vars are a uniform indexed family, not 32 named fields.
    ///
    /// Setting an override also *enables* the watch: a controller or an LFO
    /// bound to `Qvar7_0` must be able to make Q7 move without the user first
    /// adding it in the panel, which is the same "the setter is the whole
    /// control surface" rule the other families follow. `v` is in the panel's
    /// own [`crate::q_vars::Q_VAR_MIN`]..[`crate::q_vars::Q_VAR_MAX`] range,
    /// not 0..1: the registry entries convert with [`q_var_value`] first.
    fn set_q_var(&mut self, slot: usize, n: usize, v: f64);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandId {
    Crossfader,
    PresetPrevA,
    PresetNextA,
    PresetPrevB,
    PresetNextB,
    PlaylistToggleA,
    PlaylistToggleB,
    PlaylistPrevA,
    PlaylistNextA,
    PlaylistPrevB,
    PlaylistNextB,
    CrossfaderLeft,
    CrossfaderRight,
    DeckSwitch,
    PresetPrevActive,
    PresetNextActive,
    PlaylistToggleActive,
    PlaylistPrevActive,
    PlaylistNextActive,
    StrobeToggle,
    LfoRateUp,
    LfoRateDown,
    ColorHueA,
    ColorSatA,
    ColorBrightA,
    ColorContrastA,
    ColorInvertA,
    ColorHueB,
    ColorSatB,
    ColorBrightB,
    ColorContrastB,
    ColorInvertB,
    CompositeBlend0,
    CompositeBlend1,
    CompositeBlend2,
    CompositeBlend3,
    LumakeyBlack0,
    LumakeyBlack1,
    LumakeyBlack2,
    LumakeyBlack3,
    LumakeyWhite0,
    LumakeyWhite1,
    LumakeyWhite2,
    LumakeyWhite3,
    ColorkeyHue0,
    ColorkeyHue1,
    ColorkeyHue2,
    ColorkeyHue3,
    ColorkeyTolerance0,
    ColorkeyTolerance1,
    ColorkeyTolerance2,
    ColorkeyTolerance3,
    RecallSnapshot0,
    RecallSnapshot1,
    RecallSnapshot2,
    RecallSnapshot3,
    RecallSnapshot4,
    RecallSnapshot5,
    RecallSnapshot6,
    RecallSnapshot7,
    TimeSpeed0,
    TimeSpeed1,
    TimeSpeed2,
    TimeSpeed3,
    TimeZoom0,
    TimeZoom1,
    TimeZoom2,
    TimeZoom3,
    TimeRot0,
    TimeRot1,
    TimeRot2,
    TimeRot3,
    TimeWarp0,
    TimeWarp1,
    TimeWarp2,
    TimeWarp3,
    TimeDx0,
    TimeDx1,
    TimeDx2,
    TimeDx3,
    TimeDy0,
    TimeDy1,
    TimeDy2,
    TimeDy3,
    TimeStretch0,
    TimeStretch1,
    TimeStretch2,
    TimeStretch3,
    TimeWave0,
    TimeWave1,
    TimeWave2,
    TimeWave3,
    OverlayQueueNext,
    OverlayQueuePrev,
    TimelineToggle,
    Qvar1_0,
    Qvar1_1,
    Qvar1_2,
    Qvar1_3,
    Qvar2_0,
    Qvar2_1,
    Qvar2_2,
    Qvar2_3,
    Qvar3_0,
    Qvar3_1,
    Qvar3_2,
    Qvar3_3,
    Qvar4_0,
    Qvar4_1,
    Qvar4_2,
    Qvar4_3,
    Qvar5_0,
    Qvar5_1,
    Qvar5_2,
    Qvar5_3,
    Qvar6_0,
    Qvar6_1,
    Qvar6_2,
    Qvar6_3,
    Qvar7_0,
    Qvar7_1,
    Qvar7_2,
    Qvar7_3,
    Qvar8_0,
    Qvar8_1,
    Qvar8_2,
    Qvar8_3,
    Qvar9_0,
    Qvar9_1,
    Qvar9_2,
    Qvar9_3,
    Qvar10_0,
    Qvar10_1,
    Qvar10_2,
    Qvar10_3,
    Qvar11_0,
    Qvar11_1,
    Qvar11_2,
    Qvar11_3,
    Qvar12_0,
    Qvar12_1,
    Qvar12_2,
    Qvar12_3,
    Qvar13_0,
    Qvar13_1,
    Qvar13_2,
    Qvar13_3,
    Qvar14_0,
    Qvar14_1,
    Qvar14_2,
    Qvar14_3,
    Qvar15_0,
    Qvar15_1,
    Qvar15_2,
    Qvar15_3,
    Qvar16_0,
    Qvar16_1,
    Qvar16_2,
    Qvar16_3,
    Qvar17_0,
    Qvar17_1,
    Qvar17_2,
    Qvar17_3,
    Qvar18_0,
    Qvar18_1,
    Qvar18_2,
    Qvar18_3,
    Qvar19_0,
    Qvar19_1,
    Qvar19_2,
    Qvar19_3,
    Qvar20_0,
    Qvar20_1,
    Qvar20_2,
    Qvar20_3,
    Qvar21_0,
    Qvar21_1,
    Qvar21_2,
    Qvar21_3,
    Qvar22_0,
    Qvar22_1,
    Qvar22_2,
    Qvar22_3,
    Qvar23_0,
    Qvar23_1,
    Qvar23_2,
    Qvar23_3,
    Qvar24_0,
    Qvar24_1,
    Qvar24_2,
    Qvar24_3,
    Qvar25_0,
    Qvar25_1,
    Qvar25_2,
    Qvar25_3,
    Qvar26_0,
    Qvar26_1,
    Qvar26_2,
    Qvar26_3,
    Qvar27_0,
    Qvar27_1,
    Qvar27_2,
    Qvar27_3,
    Qvar28_0,
    Qvar28_1,
    Qvar28_2,
    Qvar28_3,
    Qvar29_0,
    Qvar29_1,
    Qvar29_2,
    Qvar29_3,
    Qvar30_0,
    Qvar30_1,
    Qvar30_2,
    Qvar30_3,
    Qvar31_0,
    Qvar31_1,
    Qvar31_2,
    Qvar31_3,
    Qvar32_0,
    Qvar32_1,
    Qvar32_2,
    Qvar32_3,
}

/// 'range' uses value01 (0..1); 'trigger' ignores it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandKind {
    Trigger,
    Range,
}

pub struct Command {
    pub id: CommandId,
    pub label: &'static str,
    pub kind: CommandKind,
    /// See `CommandRegistry`'s doc comment: only 2 of the 223 commands
    /// registered by `create_default_registry` still share the `noop`
    /// stub here (`CommandId::LfoRateUp`/`LfoRateDown`); this is not a
    /// signature to widen casually.
    pub run: fn(f64, &mut dyn CommandContext),
}

/// Dispatches by `CommandId` to a `Command`'s `run` fn.
///
/// Of the 223 `CommandId` variants `create_default_registry` registers,
/// only 2, `LfoRateUp`/`LfoRateDown` (Trigger), are still permanent
/// `noop` stubs. This used to describe a much larger gap (whole-branch
/// review: most `Range` commands and several `Trigger` commands stubbed
/// out pending a future dedicated phase); that gap has since been closed
/// across Steps 1-10 of the Phase 8 VJ-panels plan, which gave
/// `CommandContext` real setters for color, composite/lumakey/colorkey,
/// snapshot recall, timeline toggle, all 32 time-param multipliers, all
/// 128 q-var slots, and the strobe toggle. `run` only ever receives `&mut
/// dyn CommandContext`.
///
/// The 2 remaining stubs stay stubs: LFO routing (Step 11 of the same plan)
/// has shipped and deliberately did not wire them, because `LfoRateUp`/
/// `LfoRateDown` are not in the REQUIREMENTS' transversal-setter list:
/// LFO rate is a per-slot panel control, not a dispatchable transversal
/// parameter. This is a closed design decision, not a gap awaiting a later
/// step.
#[derive(Default)]
pub struct CommandRegistry {
    /// Insertion order matters here (whole-branch review Finding I2): 3
    /// reference UI panels, and this app's own `ui::midi`, expect `all()`
    /// to come back in the curated `DEFAULT_COMMANDS`/`default_commands()`
    /// grouping (deck controls -> active-deck shortcuts -> M2/M3 ->
    /// compositing -> snapshots -> time params -> overlay -> timeline ->
    /// q-vars), the same order the TS reference's `Map` preserved. A
    /// `HashMap<CommandId, Command>` here used to make `all()`'s order
    /// nondeterministic across runs. `commands` holds the ordered storage;
    /// `index` is a `CommandId -> position` side table for O(1) `get`/
    /// `dispatch` lookups without giving up that order.
    commands: Vec<Command>,
    index: HashMap<CommandId, usize>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self { commands: Vec::new(), index: HashMap::new() }
    }

    pub fn register(&mut self, cmd: Command) {
        match self.index.get(&cmd.id) {
            // Re-registering an id replaces it in place, preserving its
            // original position: matches the TS `Map`'s `set()` semantics
            // (an existing key keeps its insertion-order slot).
            Some(&pos) => self.commands[pos] = cmd,
            None => {
                self.index.insert(cmd.id, self.commands.len());
                self.commands.push(cmd);
            }
        }
    }

    /// Dispatch a command. value01 must be 0..1 (callers normalize MIDI 0-127 before calling).
    ///
    /// A non-finite `value01` (NaN, ±∞) is dropped here rather than handed to
    /// `run` (whole-branch review Finding I1). Every command path funnels
    /// through this method, so this one check covers all 221 real setters
    /// instead of 221 individual guards. It matters because NaN survives
    /// `f64::clamp` untouched (`clamp` is NaN-transparent), and the Time/Qvar
    /// setters end up in `engine::preset_patch::encode_param`, whose
    /// `(...).round() as i32` saturates NaN to `0`, which that channel's
    /// prologue decodes as a permanent write of `-2.0` into a live preset
    /// slot. Both remote inputs are unauthenticated (`io::osc`'s UDP socket,
    /// remote-ws) and both can carry an arbitrary `f32`/`f64`, so the value
    /// reaching here is attacker-controlled. Silently ignoring is the right
    /// shape: `dispatch` has no error channel, and every caller treats an
    /// unrecognized/undeliverable command as a no-op already (see
    /// `dispatch_on_an_unknown_id_does_not_crash`).
    pub fn dispatch(&self, id: CommandId, value01: f64, ctx: &mut dyn CommandContext) {
        if !value01.is_finite() {
            return;
        }
        if let Some(cmd) = self.get(id) {
            (cmd.run)(value01, ctx);
        }
    }

    pub fn get(&self, id: CommandId) -> Option<&Command> {
        self.index.get(&id).map(|&pos| &self.commands[pos])
    }

    /// Commands in construction/insertion order: see this struct's doc
    /// comment.
    pub fn all(&self) -> Vec<&Command> {
        self.commands.iter().collect()
    }
}

const CROSSFADER_STEP: f64 = 0.05;

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn noop(_: f64, _: &mut dyn CommandContext) {}

/// Scales a dispatched 0..1 command value onto the Time panel's own 0..2
/// slider range. Factored out rather than repeated inline in all 32
/// `CommandId::Time*` entries below.
fn time_mult(v01: f64) -> f64 {
    (v01 * crate::time_params::TIME_MULT_MAX).clamp(0.0, crate::time_params::TIME_MULT_MAX)
}

/// Scales a dispatched 0..1 command value onto the Qvar panel's own -2..2
/// slider range, so half travel on a MIDI fader lands exactly on 0. Factored
/// out rather than repeated inline in all 128 `CommandId::Qvar*` entries
/// below, the same way [`time_mult`] is.
fn q_var_value(v01: f64) -> f64 {
    crate::q_vars::clamp_q_var_value(
        crate::q_vars::Q_VAR_MIN + v01 * (crate::q_vars::Q_VAR_MAX - crate::q_vars::Q_VAR_MIN),
    )
}

pub fn create_default_registry() -> CommandRegistry {
    let mut reg = CommandRegistry::new();
    for cmd in default_commands() {
        reg.register(cmd);
    }
    reg
}

fn default_commands() -> Vec<Command> {
    vec![
        Command { id: CommandId::Crossfader, label: "Crossfader", kind: CommandKind::Range, run: |v, ctx| ctx.set_crossfader(v) },
        Command { id: CommandId::PresetPrevA, label: "◀ Preset A", kind: CommandKind::Trigger, run: |_, ctx| ctx.navigate_preset(Deck::A, -1) },
        Command { id: CommandId::PresetNextA, label: "▶ Preset A", kind: CommandKind::Trigger, run: |_, ctx| ctx.navigate_preset(Deck::A, 1) },
        Command { id: CommandId::PresetPrevB, label: "◀ Preset B", kind: CommandKind::Trigger, run: |_, ctx| ctx.navigate_preset(Deck::B, -1) },
        Command { id: CommandId::PresetNextB, label: "▶ Preset B", kind: CommandKind::Trigger, run: |_, ctx| ctx.navigate_preset(Deck::B, 1) },
        Command { id: CommandId::PlaylistToggleA, label: "⏯ Playlist A", kind: CommandKind::Trigger, run: |_, ctx| ctx.toggle_playlist(Deck::A) },
        Command { id: CommandId::PlaylistToggleB, label: "⏯ Playlist B", kind: CommandKind::Trigger, run: |_, ctx| ctx.toggle_playlist(Deck::B) },
        Command { id: CommandId::PlaylistPrevA, label: "⏮ Playlist A", kind: CommandKind::Trigger, run: |_, ctx| ctx.playlist_prev(Deck::A) },
        Command { id: CommandId::PlaylistNextA, label: "⏭ Playlist A", kind: CommandKind::Trigger, run: |_, ctx| ctx.playlist_next(Deck::A) },
        Command { id: CommandId::PlaylistPrevB, label: "⏮ Playlist B", kind: CommandKind::Trigger, run: |_, ctx| ctx.playlist_prev(Deck::B) },
        Command { id: CommandId::PlaylistNextB, label: "⏭ Playlist B", kind: CommandKind::Trigger, run: |_, ctx| ctx.playlist_next(Deck::B) },
        Command { id: CommandId::CrossfaderLeft, label: "Crossfader ←", kind: CommandKind::Trigger, run: |_, ctx| ctx.set_crossfader(round2(ctx.get_crossfader() - CROSSFADER_STEP).max(0.0)) },
        Command { id: CommandId::CrossfaderRight, label: "Crossfader →", kind: CommandKind::Trigger, run: |_, ctx| ctx.set_crossfader(round2(ctx.get_crossfader() + CROSSFADER_STEP).min(1.0)) },
        Command { id: CommandId::DeckSwitch, label: "Switch active deck", kind: CommandKind::Trigger, run: |_, ctx| ctx.switch_active_deck() },
        Command { id: CommandId::PresetPrevActive, label: "◀ Preset (active deck)", kind: CommandKind::Trigger, run: |_, ctx| { let d = ctx.get_active_deck(); ctx.navigate_preset(d, -1) } },
        Command { id: CommandId::PresetNextActive, label: "▶ Preset (active deck)", kind: CommandKind::Trigger, run: |_, ctx| { let d = ctx.get_active_deck(); ctx.navigate_preset(d, 1) } },
        Command { id: CommandId::PlaylistToggleActive, label: "⏯ Playlist (active deck)", kind: CommandKind::Trigger, run: |_, ctx| { let d = ctx.get_active_deck(); ctx.toggle_playlist(d) } },
        Command { id: CommandId::PlaylistPrevActive, label: "⏮ Playlist (active deck)", kind: CommandKind::Trigger, run: |_, ctx| { let d = ctx.get_active_deck(); ctx.playlist_prev(d) } },
        Command { id: CommandId::PlaylistNextActive, label: "⏭ Playlist (active deck)", kind: CommandKind::Trigger, run: |_, ctx| { let d = ctx.get_active_deck(); ctx.playlist_next(d) } },
        Command { id: CommandId::StrobeToggle, label: "Strobe ON/OFF", kind: CommandKind::Trigger, run: |_, ctx| ctx.toggle_strobe() },
        Command { id: CommandId::LfoRateUp, label: "LFO Rate +", kind: CommandKind::Trigger, run: noop },
        Command { id: CommandId::LfoRateDown, label: "LFO Rate −", kind: CommandKind::Trigger, run: noop },
        Command { id: CommandId::ColorHueA, label: "Hue A", kind: CommandKind::Range, run: |v, ctx| ctx.set_color_hue_a(v) },
        Command { id: CommandId::ColorSatA, label: "Saturation A", kind: CommandKind::Range, run: |v, ctx| ctx.set_color_sat_a(v) },
        Command { id: CommandId::ColorBrightA, label: "Brightness A", kind: CommandKind::Range, run: |v, ctx| ctx.set_color_bright_a(v) },
        Command { id: CommandId::ColorContrastA, label: "Contrast A", kind: CommandKind::Range, run: |v, ctx| ctx.set_color_contrast_a(v) },
        Command { id: CommandId::ColorInvertA, label: "Invert A", kind: CommandKind::Range, run: |v, ctx| ctx.set_color_invert_a(v) },
        Command { id: CommandId::ColorHueB, label: "Hue B", kind: CommandKind::Range, run: |v, ctx| ctx.set_color_hue_b(v) },
        Command { id: CommandId::ColorSatB, label: "Saturation B", kind: CommandKind::Range, run: |v, ctx| ctx.set_color_sat_b(v) },
        Command { id: CommandId::ColorBrightB, label: "Brightness B", kind: CommandKind::Range, run: |v, ctx| ctx.set_color_bright_b(v) },
        Command { id: CommandId::ColorContrastB, label: "Contrast B", kind: CommandKind::Range, run: |v, ctx| ctx.set_color_contrast_b(v) },
        Command { id: CommandId::ColorInvertB, label: "Invert B", kind: CommandKind::Range, run: |v, ctx| ctx.set_color_invert_b(v) },
        Command { id: CommandId::CompositeBlend0, label: "Blend 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_composite_blend(0, v) },
        Command { id: CommandId::CompositeBlend1, label: "Blend 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_composite_blend(1, v) },
        Command { id: CommandId::CompositeBlend2, label: "Blend 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_composite_blend(2, v) },
        Command { id: CommandId::CompositeBlend3, label: "Blend 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_composite_blend(3, v) },
        Command { id: CommandId::LumakeyBlack0, label: "Luma Black 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_composite_luma_black(0, v) },
        Command { id: CommandId::LumakeyBlack1, label: "Luma Black 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_composite_luma_black(1, v) },
        Command { id: CommandId::LumakeyBlack2, label: "Luma Black 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_composite_luma_black(2, v) },
        Command { id: CommandId::LumakeyBlack3, label: "Luma Black 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_composite_luma_black(3, v) },
        Command { id: CommandId::LumakeyWhite0, label: "Luma White 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_composite_luma_white(0, v) },
        Command { id: CommandId::LumakeyWhite1, label: "Luma White 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_composite_luma_white(1, v) },
        Command { id: CommandId::LumakeyWhite2, label: "Luma White 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_composite_luma_white(2, v) },
        Command { id: CommandId::LumakeyWhite3, label: "Luma White 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_composite_luma_white(3, v) },
        Command { id: CommandId::ColorkeyHue0, label: "Key Hue 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_composite_color_hue(0, v) },
        Command { id: CommandId::ColorkeyHue1, label: "Key Hue 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_composite_color_hue(1, v) },
        Command { id: CommandId::ColorkeyHue2, label: "Key Hue 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_composite_color_hue(2, v) },
        Command { id: CommandId::ColorkeyHue3, label: "Key Hue 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_composite_color_hue(3, v) },
        Command { id: CommandId::ColorkeyTolerance0, label: "Key Tolerance 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_composite_color_tol(0, v) },
        Command { id: CommandId::ColorkeyTolerance1, label: "Key Tolerance 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_composite_color_tol(1, v) },
        Command { id: CommandId::ColorkeyTolerance2, label: "Key Tolerance 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_composite_color_tol(2, v) },
        Command { id: CommandId::ColorkeyTolerance3, label: "Key Tolerance 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_composite_color_tol(3, v) },
        Command { id: CommandId::RecallSnapshot0, label: "Recall Snapshot 0", kind: CommandKind::Trigger, run: |_, ctx| ctx.recall_snapshot(0) },
        Command { id: CommandId::RecallSnapshot1, label: "Recall Snapshot 1", kind: CommandKind::Trigger, run: |_, ctx| ctx.recall_snapshot(1) },
        Command { id: CommandId::RecallSnapshot2, label: "Recall Snapshot 2", kind: CommandKind::Trigger, run: |_, ctx| ctx.recall_snapshot(2) },
        Command { id: CommandId::RecallSnapshot3, label: "Recall Snapshot 3", kind: CommandKind::Trigger, run: |_, ctx| ctx.recall_snapshot(3) },
        Command { id: CommandId::RecallSnapshot4, label: "Recall Snapshot 4", kind: CommandKind::Trigger, run: |_, ctx| ctx.recall_snapshot(4) },
        Command { id: CommandId::RecallSnapshot5, label: "Recall Snapshot 5", kind: CommandKind::Trigger, run: |_, ctx| ctx.recall_snapshot(5) },
        Command { id: CommandId::RecallSnapshot6, label: "Recall Snapshot 6", kind: CommandKind::Trigger, run: |_, ctx| ctx.recall_snapshot(6) },
        Command { id: CommandId::RecallSnapshot7, label: "Recall Snapshot 7", kind: CommandKind::Trigger, run: |_, ctx| ctx.recall_snapshot(7) },
        Command { id: CommandId::TimeSpeed0, label: "Speed 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_time_speed(0, time_mult(v)) },
        Command { id: CommandId::TimeSpeed1, label: "Speed 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_time_speed(1, time_mult(v)) },
        Command { id: CommandId::TimeSpeed2, label: "Speed 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_time_speed(2, time_mult(v)) },
        Command { id: CommandId::TimeSpeed3, label: "Speed 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_time_speed(3, time_mult(v)) },
        Command { id: CommandId::TimeZoom0, label: "Zoom 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_time_zoom(0, time_mult(v)) },
        Command { id: CommandId::TimeZoom1, label: "Zoom 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_time_zoom(1, time_mult(v)) },
        Command { id: CommandId::TimeZoom2, label: "Zoom 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_time_zoom(2, time_mult(v)) },
        Command { id: CommandId::TimeZoom3, label: "Zoom 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_time_zoom(3, time_mult(v)) },
        Command { id: CommandId::TimeRot0, label: "Rotation 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_time_rot(0, time_mult(v)) },
        Command { id: CommandId::TimeRot1, label: "Rotation 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_time_rot(1, time_mult(v)) },
        Command { id: CommandId::TimeRot2, label: "Rotation 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_time_rot(2, time_mult(v)) },
        Command { id: CommandId::TimeRot3, label: "Rotation 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_time_rot(3, time_mult(v)) },
        Command { id: CommandId::TimeWarp0, label: "Wrap 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_time_warp(0, time_mult(v)) },
        Command { id: CommandId::TimeWarp1, label: "Wrap 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_time_warp(1, time_mult(v)) },
        Command { id: CommandId::TimeWarp2, label: "Wrap 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_time_warp(2, time_mult(v)) },
        Command { id: CommandId::TimeWarp3, label: "Wrap 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_time_warp(3, time_mult(v)) },
        Command { id: CommandId::TimeDx0, label: "Horizontal 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_time_dx(0, time_mult(v)) },
        Command { id: CommandId::TimeDx1, label: "Horizontal 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_time_dx(1, time_mult(v)) },
        Command { id: CommandId::TimeDx2, label: "Horizontal 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_time_dx(2, time_mult(v)) },
        Command { id: CommandId::TimeDx3, label: "Horizontal 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_time_dx(3, time_mult(v)) },
        Command { id: CommandId::TimeDy0, label: "Vertical 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_time_dy(0, time_mult(v)) },
        Command { id: CommandId::TimeDy1, label: "Vertical 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_time_dy(1, time_mult(v)) },
        Command { id: CommandId::TimeDy2, label: "Vertical 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_time_dy(2, time_mult(v)) },
        Command { id: CommandId::TimeDy3, label: "Vertical 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_time_dy(3, time_mult(v)) },
        Command { id: CommandId::TimeStretch0, label: "Stretch 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_time_stretch(0, time_mult(v)) },
        Command { id: CommandId::TimeStretch1, label: "Stretch 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_time_stretch(1, time_mult(v)) },
        Command { id: CommandId::TimeStretch2, label: "Stretch 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_time_stretch(2, time_mult(v)) },
        Command { id: CommandId::TimeStretch3, label: "Stretch 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_time_stretch(3, time_mult(v)) },
        Command { id: CommandId::TimeWave0, label: "Wave 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_time_wave(0, time_mult(v)) },
        Command { id: CommandId::TimeWave1, label: "Wave 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_time_wave(1, time_mult(v)) },
        Command { id: CommandId::TimeWave2, label: "Wave 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_time_wave(2, time_mult(v)) },
        Command { id: CommandId::TimeWave3, label: "Wave 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_time_wave(3, time_mult(v)) },
        Command { id: CommandId::OverlayQueueNext, label: "Overlay Queue Next", kind: CommandKind::Trigger, run: |_, ctx| ctx.advance_overlay_queue(1) },
        Command { id: CommandId::OverlayQueuePrev, label: "Overlay Queue Prev", kind: CommandKind::Trigger, run: |_, ctx| ctx.advance_overlay_queue(-1) },
        Command { id: CommandId::TimelineToggle, label: "Timeline Play/Pause", kind: CommandKind::Trigger, run: |_, ctx| ctx.toggle_timeline() },
        Command { id: CommandId::Qvar1_0, label: "Q1 Deck 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(0, 1, q_var_value(v)) },
        Command { id: CommandId::Qvar1_1, label: "Q1 Deck 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(1, 1, q_var_value(v)) },
        Command { id: CommandId::Qvar1_2, label: "Q1 Deck 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(2, 1, q_var_value(v)) },
        Command { id: CommandId::Qvar1_3, label: "Q1 Deck 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(3, 1, q_var_value(v)) },
        Command { id: CommandId::Qvar2_0, label: "Q2 Deck 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(0, 2, q_var_value(v)) },
        Command { id: CommandId::Qvar2_1, label: "Q2 Deck 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(1, 2, q_var_value(v)) },
        Command { id: CommandId::Qvar2_2, label: "Q2 Deck 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(2, 2, q_var_value(v)) },
        Command { id: CommandId::Qvar2_3, label: "Q2 Deck 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(3, 2, q_var_value(v)) },
        Command { id: CommandId::Qvar3_0, label: "Q3 Deck 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(0, 3, q_var_value(v)) },
        Command { id: CommandId::Qvar3_1, label: "Q3 Deck 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(1, 3, q_var_value(v)) },
        Command { id: CommandId::Qvar3_2, label: "Q3 Deck 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(2, 3, q_var_value(v)) },
        Command { id: CommandId::Qvar3_3, label: "Q3 Deck 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(3, 3, q_var_value(v)) },
        Command { id: CommandId::Qvar4_0, label: "Q4 Deck 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(0, 4, q_var_value(v)) },
        Command { id: CommandId::Qvar4_1, label: "Q4 Deck 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(1, 4, q_var_value(v)) },
        Command { id: CommandId::Qvar4_2, label: "Q4 Deck 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(2, 4, q_var_value(v)) },
        Command { id: CommandId::Qvar4_3, label: "Q4 Deck 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(3, 4, q_var_value(v)) },
        Command { id: CommandId::Qvar5_0, label: "Q5 Deck 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(0, 5, q_var_value(v)) },
        Command { id: CommandId::Qvar5_1, label: "Q5 Deck 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(1, 5, q_var_value(v)) },
        Command { id: CommandId::Qvar5_2, label: "Q5 Deck 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(2, 5, q_var_value(v)) },
        Command { id: CommandId::Qvar5_3, label: "Q5 Deck 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(3, 5, q_var_value(v)) },
        Command { id: CommandId::Qvar6_0, label: "Q6 Deck 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(0, 6, q_var_value(v)) },
        Command { id: CommandId::Qvar6_1, label: "Q6 Deck 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(1, 6, q_var_value(v)) },
        Command { id: CommandId::Qvar6_2, label: "Q6 Deck 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(2, 6, q_var_value(v)) },
        Command { id: CommandId::Qvar6_3, label: "Q6 Deck 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(3, 6, q_var_value(v)) },
        Command { id: CommandId::Qvar7_0, label: "Q7 Deck 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(0, 7, q_var_value(v)) },
        Command { id: CommandId::Qvar7_1, label: "Q7 Deck 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(1, 7, q_var_value(v)) },
        Command { id: CommandId::Qvar7_2, label: "Q7 Deck 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(2, 7, q_var_value(v)) },
        Command { id: CommandId::Qvar7_3, label: "Q7 Deck 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(3, 7, q_var_value(v)) },
        Command { id: CommandId::Qvar8_0, label: "Q8 Deck 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(0, 8, q_var_value(v)) },
        Command { id: CommandId::Qvar8_1, label: "Q8 Deck 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(1, 8, q_var_value(v)) },
        Command { id: CommandId::Qvar8_2, label: "Q8 Deck 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(2, 8, q_var_value(v)) },
        Command { id: CommandId::Qvar8_3, label: "Q8 Deck 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(3, 8, q_var_value(v)) },
        Command { id: CommandId::Qvar9_0, label: "Q9 Deck 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(0, 9, q_var_value(v)) },
        Command { id: CommandId::Qvar9_1, label: "Q9 Deck 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(1, 9, q_var_value(v)) },
        Command { id: CommandId::Qvar9_2, label: "Q9 Deck 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(2, 9, q_var_value(v)) },
        Command { id: CommandId::Qvar9_3, label: "Q9 Deck 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(3, 9, q_var_value(v)) },
        Command { id: CommandId::Qvar10_0, label: "Q10 Deck 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(0, 10, q_var_value(v)) },
        Command { id: CommandId::Qvar10_1, label: "Q10 Deck 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(1, 10, q_var_value(v)) },
        Command { id: CommandId::Qvar10_2, label: "Q10 Deck 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(2, 10, q_var_value(v)) },
        Command { id: CommandId::Qvar10_3, label: "Q10 Deck 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(3, 10, q_var_value(v)) },
        Command { id: CommandId::Qvar11_0, label: "Q11 Deck 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(0, 11, q_var_value(v)) },
        Command { id: CommandId::Qvar11_1, label: "Q11 Deck 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(1, 11, q_var_value(v)) },
        Command { id: CommandId::Qvar11_2, label: "Q11 Deck 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(2, 11, q_var_value(v)) },
        Command { id: CommandId::Qvar11_3, label: "Q11 Deck 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(3, 11, q_var_value(v)) },
        Command { id: CommandId::Qvar12_0, label: "Q12 Deck 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(0, 12, q_var_value(v)) },
        Command { id: CommandId::Qvar12_1, label: "Q12 Deck 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(1, 12, q_var_value(v)) },
        Command { id: CommandId::Qvar12_2, label: "Q12 Deck 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(2, 12, q_var_value(v)) },
        Command { id: CommandId::Qvar12_3, label: "Q12 Deck 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(3, 12, q_var_value(v)) },
        Command { id: CommandId::Qvar13_0, label: "Q13 Deck 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(0, 13, q_var_value(v)) },
        Command { id: CommandId::Qvar13_1, label: "Q13 Deck 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(1, 13, q_var_value(v)) },
        Command { id: CommandId::Qvar13_2, label: "Q13 Deck 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(2, 13, q_var_value(v)) },
        Command { id: CommandId::Qvar13_3, label: "Q13 Deck 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(3, 13, q_var_value(v)) },
        Command { id: CommandId::Qvar14_0, label: "Q14 Deck 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(0, 14, q_var_value(v)) },
        Command { id: CommandId::Qvar14_1, label: "Q14 Deck 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(1, 14, q_var_value(v)) },
        Command { id: CommandId::Qvar14_2, label: "Q14 Deck 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(2, 14, q_var_value(v)) },
        Command { id: CommandId::Qvar14_3, label: "Q14 Deck 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(3, 14, q_var_value(v)) },
        Command { id: CommandId::Qvar15_0, label: "Q15 Deck 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(0, 15, q_var_value(v)) },
        Command { id: CommandId::Qvar15_1, label: "Q15 Deck 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(1, 15, q_var_value(v)) },
        Command { id: CommandId::Qvar15_2, label: "Q15 Deck 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(2, 15, q_var_value(v)) },
        Command { id: CommandId::Qvar15_3, label: "Q15 Deck 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(3, 15, q_var_value(v)) },
        Command { id: CommandId::Qvar16_0, label: "Q16 Deck 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(0, 16, q_var_value(v)) },
        Command { id: CommandId::Qvar16_1, label: "Q16 Deck 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(1, 16, q_var_value(v)) },
        Command { id: CommandId::Qvar16_2, label: "Q16 Deck 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(2, 16, q_var_value(v)) },
        Command { id: CommandId::Qvar16_3, label: "Q16 Deck 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(3, 16, q_var_value(v)) },
        Command { id: CommandId::Qvar17_0, label: "Q17 Deck 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(0, 17, q_var_value(v)) },
        Command { id: CommandId::Qvar17_1, label: "Q17 Deck 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(1, 17, q_var_value(v)) },
        Command { id: CommandId::Qvar17_2, label: "Q17 Deck 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(2, 17, q_var_value(v)) },
        Command { id: CommandId::Qvar17_3, label: "Q17 Deck 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(3, 17, q_var_value(v)) },
        Command { id: CommandId::Qvar18_0, label: "Q18 Deck 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(0, 18, q_var_value(v)) },
        Command { id: CommandId::Qvar18_1, label: "Q18 Deck 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(1, 18, q_var_value(v)) },
        Command { id: CommandId::Qvar18_2, label: "Q18 Deck 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(2, 18, q_var_value(v)) },
        Command { id: CommandId::Qvar18_3, label: "Q18 Deck 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(3, 18, q_var_value(v)) },
        Command { id: CommandId::Qvar19_0, label: "Q19 Deck 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(0, 19, q_var_value(v)) },
        Command { id: CommandId::Qvar19_1, label: "Q19 Deck 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(1, 19, q_var_value(v)) },
        Command { id: CommandId::Qvar19_2, label: "Q19 Deck 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(2, 19, q_var_value(v)) },
        Command { id: CommandId::Qvar19_3, label: "Q19 Deck 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(3, 19, q_var_value(v)) },
        Command { id: CommandId::Qvar20_0, label: "Q20 Deck 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(0, 20, q_var_value(v)) },
        Command { id: CommandId::Qvar20_1, label: "Q20 Deck 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(1, 20, q_var_value(v)) },
        Command { id: CommandId::Qvar20_2, label: "Q20 Deck 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(2, 20, q_var_value(v)) },
        Command { id: CommandId::Qvar20_3, label: "Q20 Deck 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(3, 20, q_var_value(v)) },
        Command { id: CommandId::Qvar21_0, label: "Q21 Deck 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(0, 21, q_var_value(v)) },
        Command { id: CommandId::Qvar21_1, label: "Q21 Deck 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(1, 21, q_var_value(v)) },
        Command { id: CommandId::Qvar21_2, label: "Q21 Deck 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(2, 21, q_var_value(v)) },
        Command { id: CommandId::Qvar21_3, label: "Q21 Deck 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(3, 21, q_var_value(v)) },
        Command { id: CommandId::Qvar22_0, label: "Q22 Deck 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(0, 22, q_var_value(v)) },
        Command { id: CommandId::Qvar22_1, label: "Q22 Deck 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(1, 22, q_var_value(v)) },
        Command { id: CommandId::Qvar22_2, label: "Q22 Deck 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(2, 22, q_var_value(v)) },
        Command { id: CommandId::Qvar22_3, label: "Q22 Deck 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(3, 22, q_var_value(v)) },
        Command { id: CommandId::Qvar23_0, label: "Q23 Deck 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(0, 23, q_var_value(v)) },
        Command { id: CommandId::Qvar23_1, label: "Q23 Deck 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(1, 23, q_var_value(v)) },
        Command { id: CommandId::Qvar23_2, label: "Q23 Deck 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(2, 23, q_var_value(v)) },
        Command { id: CommandId::Qvar23_3, label: "Q23 Deck 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(3, 23, q_var_value(v)) },
        Command { id: CommandId::Qvar24_0, label: "Q24 Deck 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(0, 24, q_var_value(v)) },
        Command { id: CommandId::Qvar24_1, label: "Q24 Deck 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(1, 24, q_var_value(v)) },
        Command { id: CommandId::Qvar24_2, label: "Q24 Deck 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(2, 24, q_var_value(v)) },
        Command { id: CommandId::Qvar24_3, label: "Q24 Deck 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(3, 24, q_var_value(v)) },
        Command { id: CommandId::Qvar25_0, label: "Q25 Deck 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(0, 25, q_var_value(v)) },
        Command { id: CommandId::Qvar25_1, label: "Q25 Deck 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(1, 25, q_var_value(v)) },
        Command { id: CommandId::Qvar25_2, label: "Q25 Deck 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(2, 25, q_var_value(v)) },
        Command { id: CommandId::Qvar25_3, label: "Q25 Deck 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(3, 25, q_var_value(v)) },
        Command { id: CommandId::Qvar26_0, label: "Q26 Deck 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(0, 26, q_var_value(v)) },
        Command { id: CommandId::Qvar26_1, label: "Q26 Deck 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(1, 26, q_var_value(v)) },
        Command { id: CommandId::Qvar26_2, label: "Q26 Deck 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(2, 26, q_var_value(v)) },
        Command { id: CommandId::Qvar26_3, label: "Q26 Deck 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(3, 26, q_var_value(v)) },
        Command { id: CommandId::Qvar27_0, label: "Q27 Deck 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(0, 27, q_var_value(v)) },
        Command { id: CommandId::Qvar27_1, label: "Q27 Deck 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(1, 27, q_var_value(v)) },
        Command { id: CommandId::Qvar27_2, label: "Q27 Deck 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(2, 27, q_var_value(v)) },
        Command { id: CommandId::Qvar27_3, label: "Q27 Deck 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(3, 27, q_var_value(v)) },
        Command { id: CommandId::Qvar28_0, label: "Q28 Deck 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(0, 28, q_var_value(v)) },
        Command { id: CommandId::Qvar28_1, label: "Q28 Deck 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(1, 28, q_var_value(v)) },
        Command { id: CommandId::Qvar28_2, label: "Q28 Deck 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(2, 28, q_var_value(v)) },
        Command { id: CommandId::Qvar28_3, label: "Q28 Deck 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(3, 28, q_var_value(v)) },
        Command { id: CommandId::Qvar29_0, label: "Q29 Deck 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(0, 29, q_var_value(v)) },
        Command { id: CommandId::Qvar29_1, label: "Q29 Deck 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(1, 29, q_var_value(v)) },
        Command { id: CommandId::Qvar29_2, label: "Q29 Deck 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(2, 29, q_var_value(v)) },
        Command { id: CommandId::Qvar29_3, label: "Q29 Deck 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(3, 29, q_var_value(v)) },
        Command { id: CommandId::Qvar30_0, label: "Q30 Deck 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(0, 30, q_var_value(v)) },
        Command { id: CommandId::Qvar30_1, label: "Q30 Deck 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(1, 30, q_var_value(v)) },
        Command { id: CommandId::Qvar30_2, label: "Q30 Deck 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(2, 30, q_var_value(v)) },
        Command { id: CommandId::Qvar30_3, label: "Q30 Deck 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(3, 30, q_var_value(v)) },
        Command { id: CommandId::Qvar31_0, label: "Q31 Deck 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(0, 31, q_var_value(v)) },
        Command { id: CommandId::Qvar31_1, label: "Q31 Deck 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(1, 31, q_var_value(v)) },
        Command { id: CommandId::Qvar31_2, label: "Q31 Deck 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(2, 31, q_var_value(v)) },
        Command { id: CommandId::Qvar31_3, label: "Q31 Deck 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(3, 31, q_var_value(v)) },
        Command { id: CommandId::Qvar32_0, label: "Q32 Deck 0", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(0, 32, q_var_value(v)) },
        Command { id: CommandId::Qvar32_1, label: "Q32 Deck 1", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(1, 32, q_var_value(v)) },
        Command { id: CommandId::Qvar32_2, label: "Q32 Deck 2", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(2, 32, q_var_value(v)) },
        Command { id: CommandId::Qvar32_3, label: "Q32 Deck 3", kind: CommandKind::Range, run: |v, ctx| ctx.set_q_var(3, 32, q_var_value(v)) },
    ]
}

#[cfg(test)]
mod tests {
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
            // Finding I1 regression test: NaN/±∞ arriving from an
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
            // Finding I2 regression test: register a batch of ids that would
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
            // Finding I2: `all()`'s order must match `default_commands()`'s
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
}
