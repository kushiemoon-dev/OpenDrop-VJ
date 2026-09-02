//! Live show state driving the compositor: crossfader, per-deck bus
//! assignment, per-slot composite config, per-bus color params. Pure
//! state/logic: no GL, no I/O. Implements `commands::CommandContext` so
//! the keyboard dispatch (`app::keymap` + `commands::create_default_registry`)
//! can drive it directly.
//!
//! `bus_gain` and the default bus assignment are ported from OpenDrop-VJ
//! `src/routes/+page.svelte:264-269`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::beat_detector::BeatDetector;
use crate::beat_trigger::{
    default_beat_trigger_config, default_volume_peak_state, detect_volume_peak, should_trigger_on_beat,
    BeatTriggerConfig, BeatTriggerMode, VolumePeakState,
};
use crate::blend::{
    blend_mode_from_value01, blend_mode_to_value01, ColorParams, SlotComposite, DEFAULT_COLOR_PARAMS,
    DEFAULT_SLOT_COMPOSITE,
};
use crate::clock::Clock;
use crate::commands::{CommandContext, CommandId, Deck};
use crate::playlist::{PlaylistEngine, PlaylistMode, PlaylistStore};
use crate::preset_index::PresetMeta;
use crate::q_vars::{clamp_q_var_value, default_q_var_params, with_q_var_value, with_q_var_watch, QVarParamsTuple};
use crate::snapshot::{tick_active_recall, ActiveRecall, Snapshot};
use crate::time_params::{clamp_time_mult, DeckTimeParams};
use crate::timeline::{timeline_loop_duration, timeline_values_at, TimelineKeyframe};

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
    /// The 8 Time multipliers per deck slot (Step 8 of the Phase 8 plan),
    /// written by the Time panel and by the 32 `CommandId::Time*` setters.
    /// Read by `app`'s per-frame push loop, which forwards the changed ones
    /// into each deck's running preset: `Show` itself stays engine-free.
    pub time_params: [DeckTimeParams; 4],
    /// The 32 q-var overrides per deck slot (Step 9 of the Phase 8 plan),
    /// written by the Qvar panel and by the 128 `CommandId::Qvar*` setters.
    /// Read by `app` on two different cadences, both engine-free from here:
    /// changed *values* go into the running preset through the same
    /// one-word-per-frame side channel Time uses, while a change to the set
    /// of `enabled` watches needs the deck's preset re-patched and reloaded
    /// (see `engine::qvar_patch`).
    pub q_var_params: QVarParamsTuple,
    pub preset_catalog: Vec<PresetMeta>,
    preset_index_a: usize,
    preset_index_b: usize,
    fired_preset_a: Rc<RefCell<Option<String>>>,
    fired_preset_b: Rc<RefCell<Option<String>>>,
    pub playlists: PlaylistStore,
    pub lock_a: bool,
    pub lock_b: bool,
    pub clock: Clock,
    pub beat_detector: BeatDetector,
    pub beat_sync_a: bool,
    pub beat_sync_b: bool,
    pub beat_trigger_a: BeatTriggerConfig,
    pub beat_trigger_b: BeatTriggerConfig,
    /// The 8 snapshot slots (Step 4 of the Phase 8 plan). `None` = empty
    /// slot. Populated by the snapshot panel's Save button
    /// (`capture_snapshot_values`), consumed by `recall_snapshot`/
    /// `tick_recall`.
    pub snapshot_slots: [Option<Snapshot>; 8],
    /// How long a snapshot recall takes to fully interpolate, in seconds.
    /// Slider range 0.1-10s (snapshot panel).
    pub snapshot_recall_duration_sec: f64,
    /// The in-progress recall, if any: armed by `recall_snapshot`,
    /// advanced each render tick by `tick_recall`.
    pub active_recall: Option<ActiveRecall>,
    /// Up to 8 keyframes (Step 5 of the Phase 8 plan) sequencing playback
    /// across the existing snapshot slots. Kept sorted by `time_sec` by
    /// the timeline panel (`app::ui::timeline`): `timeline_values_at`
    /// assumes sorted input.
    pub timeline_keyframes: Vec<TimelineKeyframe>,
    /// Whether the timeline loop is currently advancing. Toggled through
    /// `CommandContext::toggle_timeline` (keyboard/MIDI/OSC/remote-ws
    /// parity), advanced each render tick by `tick_timeline`.
    pub timeline_playing: bool,
    /// Seconds elapsed since timeline playback last started, accumulated
    /// by `tick_timeline` from caller-supplied dt: same "no wall clock of
    /// its own" convention as `tick_recall` (see that method's doc
    /// comment). Reset to 0 by `toggle_timeline` on every transition to
    /// `true`, so play always restarts at the beginning of the current
    /// loop cycle rather than resuming stale progress or jumping in time.
    timeline_elapsed_sec: f64,
    pub auto_xfade: bool,
    /// Cadence of the auto-crossfade, in beats: DISTINCT from
    /// `beat_trigger_a/b.beats_per_change` (per-deck playlist-advance
    /// cadence). Two separate fields in `beat-sync-store.svelte.ts:25-27`.
    pub beats_per_change: u32,
    auto_xfade_count: u32,
    /// 0.0 = no manual BPM set.
    pub manual_bpm: f64,
    tap_times: Vec<f64>,
    volume_peak_state_a: VolumePeakState,
    volume_peak_state_b: VolumePeakState,
}

impl Default for Show {
    fn default() -> Self {
        // The PlaylistEngine on_preset closures can't re-borrow &mut Show to
        // write into fired_preset_a/fired_preset_b (PlaylistStore's methods
        // are already &mut self on show.playlists), so they close over
        // clones of the Rc instead, made before PlaylistStore is built.
        let fired_preset_a = Rc::new(RefCell::new(None));
        let fired_preset_b = Rc::new(RefCell::new(None));
        let (cell_a, cell_b) = (fired_preset_a.clone(), fired_preset_b.clone());
        let mut playlists = PlaylistStore::new();
        playlists.set_engines(
            PlaylistEngine::new(
                Vec::new(),
                PlaylistMode::Sequential,
                10_000.0,
                Box::new(move |name| *cell_a.borrow_mut() = Some(name.to_string())),
            ),
            PlaylistEngine::new(
                Vec::new(),
                PlaylistMode::Sequential,
                10_000.0,
                Box::new(move |name| *cell_b.borrow_mut() = Some(name.to_string())),
            ),
        );
        Self {
            crossfader: 0.0,
            deck_bus: [DeckBus::A, DeckBus::B, DeckBus::Off, DeckBus::Off],
            active_deck: Deck::A,
            selected_slot: 0,
            slot_composites: [DEFAULT_SLOT_COMPOSITE; 4],
            color_params_a: DEFAULT_COLOR_PARAMS,
            color_params_b: DEFAULT_COLOR_PARAMS,
            time_params: [DeckTimeParams::default(); 4],
            q_var_params: [default_q_var_params(); 4],
            preset_catalog: Vec::new(),
            preset_index_a: 0,
            preset_index_b: 0,
            fired_preset_a,
            fired_preset_b,
            playlists,
            lock_a: false,
            lock_b: false,
            clock: Clock::new(),
            beat_detector: BeatDetector::new(),
            beat_sync_a: false,
            beat_sync_b: false,
            beat_trigger_a: default_beat_trigger_config(),
            beat_trigger_b: default_beat_trigger_config(),
            snapshot_slots: std::array::from_fn(|_| None),
            snapshot_recall_duration_sec: 1.0,
            active_recall: None,
            timeline_keyframes: Vec::new(),
            timeline_playing: false,
            timeline_elapsed_sec: 0.0,
            auto_xfade: false,
            beats_per_change: 8,
            auto_xfade_count: 0,
            manual_bpm: 0.0,
            tap_times: Vec::new(),
            volume_peak_state_a: default_volume_peak_state(),
            volume_peak_state_b: default_volume_peak_state(),
        }
    }
}

/// A preset name chosen by `navigate_preset`, resolved to the physical slot
/// its bus letter maps to. Drained from `Show::take_fired_presets` by `app`
/// each tick to trigger the validated load (`Deck::load_preset`): `Show`
/// stays I/O-free and never touches GL directly.
pub struct PendingPresetLoad {
    pub slot: usize,
    pub name: String,
}

impl Show {
    /// Reseeds both playlist engines' shuffle-mode RNGs with real
    /// per-launch entropy supplied by the caller (`app`'s bootstrap:
    /// `core` stays zero-I/O and has no clock of its own). Deck A and B get
    /// distinct-but-derived seeds so they don't draw identical shuffle
    /// sequences from the same source entropy. Whole-branch review Finding
    /// I4: without this, shuffle mode replayed the exact same sequence
    /// every single app launch.
    pub fn reseed_rng(&mut self, seed: u64) {
        if let Some(engine) = self.playlists.engine_a_mut() {
            engine.reseed_rng(seed);
        }
        if let Some(engine) = self.playlists.engine_b_mut() {
            engine.reseed_rng(seed ^ 0xA5A5_A5A5_A5A5_A5A5);
        }
    }

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

    /// Drains presets fired by `navigate_preset` since the last drain,
    /// resolved to their physical slot. If a deck's letter isn't assigned to
    /// any slot (both `Off`, or both slots on the other letter), the fired
    /// preset is silently dropped: consistent with "Active" shortcuts
    /// having no visible effect when that deck isn't displayed anywhere.
    pub fn take_fired_presets(&mut self) -> Vec<PendingPresetLoad> {
        let mut out = Vec::new();
        for (deck, cell) in [(Deck::A, &self.fired_preset_a), (Deck::B, &self.fired_preset_b)] {
            if let Some(name) = cell.borrow_mut().take() {
                if let Some(slot) = self.deck_bus_slot_for(deck) {
                    out.push(PendingPresetLoad { slot, name });
                }
            }
        }
        out
    }

    /// Called by `app` once per beat emitted by `clock`/`beat_detector` (see
    /// step 18 for the call site). Port of `onBeat`,
    /// `beat-tempo-actions.ts:48-70` (minus the overlay/video/network pulse,
    /// out of scope here).
    pub fn on_beat(&mut self) {
        if self.auto_xfade {
            self.auto_xfade_count = (self.auto_xfade_count + 1) % self.beats_per_change.max(1);
            if self.auto_xfade_count == 0 {
                self.crossfader = if self.crossfader < 0.5 { 1.0 } else { 0.0 };
            }
        }
        self.maybe_advance_on_beat(Deck::A);
        self.maybe_advance_on_beat(Deck::B);
    }

    /// Restarts the auto-crossfade cadence from the top. Port of the
    /// unconditional `resetAutoXfadeCount()` call the TS reference makes on
    /// every auto-xfade toggle, either direction (`+page.svelte:1754`).
    /// Whole-branch review Finding 7: `app` never called an equivalent of
    /// this: toggling auto-xfade off then back on resumed the crossfade
    /// cycle mid-count instead of restarting it, so the first crossfade
    /// after re-enabling could land 1-7 beats early/late.
    pub fn reset_auto_xfade_count(&mut self) {
        self.auto_xfade_count = 0;
    }

    fn maybe_advance_on_beat(&mut self, deck: Deck) {
        let (synced, locked, trigger) = match deck {
            Deck::A => (self.beat_sync_a, self.lock_a, self.beat_trigger_a),
            Deck::B => (self.beat_sync_b, self.lock_b, self.beat_trigger_b),
        };
        if !synced || locked {
            return;
        }
        if !should_trigger_on_beat(self.clock.beat_count() as i64, trigger) {
            return;
        }
        self.advance_or_navigate(deck);
    }

    /// Advances the deck's playlist if it has items, otherwise falls back to
    /// cycling the full preset catalog via `navigate_preset`.
    fn advance_or_navigate(&mut self, deck: Deck) {
        let has_items = match deck {
            Deck::A => !self.playlists.a_items.is_empty(),
            Deck::B => !self.playlists.b_items.is_empty(),
        };
        if has_items {
            self.playlists.playlist_next(deck);
        } else {
            self.navigate_preset(deck, 1);
        }
    }

    /// Drives the interval half of the two playlist engines, once per
    /// render tick, the way `on_beat` drives the beat half. `dt_ms` is the
    /// same elapsed time `Clock::step` is fed, in milliseconds.
    ///
    /// Both engines' intervals are re-derived from the live state on every
    /// tick, rather than only when `toggle_playlist`/`toggle_beat_sync`
    /// runs: that is what makes the panel's "Interval (s)" slider take
    /// effect on a deck that is already playing, and it re-asserts the
    /// infinite (fully beat-driven) interval on a beat-synced deck that
    /// `toggle_playlist` would otherwise have reset to a finite one.
    pub fn tick_playlists(&mut self, dt_ms: f64) {
        let interval_ms = self.playlists.interval_sec * 1000.0;
        for (deck, synced) in [(Deck::A, self.beat_sync_a), (Deck::B, self.beat_sync_b)] {
            self.playlists.set_beat_sync_interval(deck, if synced { f64::INFINITY } else { interval_ms });
        }
        self.playlists.tick(dt_ms);
    }

    /// Port of `toggleBeatSync`, `beat-tempo-actions.ts:83-97`: flips the
    /// deck's beat-sync flag and switches the playlist engine's own timer
    /// between infinite (fully beat-driven) and the configured interval.
    pub fn toggle_beat_sync(&mut self, deck: Deck) {
        let synced = match deck {
            Deck::A => {
                self.beat_sync_a = !self.beat_sync_a;
                self.beat_sync_a
            }
            Deck::B => {
                self.beat_sync_b = !self.beat_sync_b;
                self.beat_sync_b
            }
        };
        let ms = if synced { f64::INFINITY } else { self.playlists.interval_sec * 1000.0 };
        self.playlists.set_beat_sync_interval(deck, ms);
    }

    /// Port of `tapTempo`, `beat-tempo-actions.ts:99-111`. `now_ms` is
    /// supplied by `app`: `core` owns no real clock.
    pub fn tap_tempo(&mut self, now_ms: f64) {
        self.tap_times.push(now_ms);
        if self.tap_times.len() > 4 {
            self.tap_times.remove(0);
        }
        if self.tap_times.len() < 2 {
            return;
        }
        let avg = self.tap_times.windows(2).map(|w| w[1] - w[0]).sum::<f64>() / (self.tap_times.len() - 1) as f64;
        let bpm = (60_000.0 / avg).round();
        if !(40.0..=300.0).contains(&bpm) {
            return;
        }
        self.manual_bpm = bpm;
        self.clock.set_bpm(bpm);
        self.clock.pulse(None); // resync phase only, bpm already set: does not emit a beat.
    }

    /// Port of `clearManualBpm`, `beat-tempo-actions.ts:113-117`.
    pub fn clear_manual_bpm(&mut self) {
        self.manual_bpm = 0.0;
        self.tap_times.clear();
        self.clock.set_bpm(0.0);
    }

    /// Manual BPM if set, otherwise the detected BPM (`0.0` if neither is
    /// available: the UI shows "—" on `0.0`, port of the ternary in
    /// `SidebarPlaylist.svelte:113`).
    pub fn current_bpm(&self) -> f64 {
        if self.manual_bpm > 0.0 {
            self.manual_bpm
        } else {
            self.beat_detector.bpm()
        }
    }

    /// Trigger independent of the beat count, for `BeatTriggerMode::VolumePeak`
    /// (`should_trigger_on_beat` explicitly ignores that mode). `rms` is
    /// supplied by `app`: see step 9's `audio::analysis::vu_level`.
    pub fn check_volume_peak_triggers(&mut self, rms: f64, now_ms: f64) {
        self.check_one_volume_peak(Deck::A, rms, now_ms);
        self.check_one_volume_peak(Deck::B, rms, now_ms);
    }

    fn check_one_volume_peak(&mut self, deck: Deck, rms: f64, now_ms: f64) {
        let (synced, locked, trigger, state) = match deck {
            Deck::A => (self.beat_sync_a, self.lock_a, self.beat_trigger_a, self.volume_peak_state_a),
            Deck::B => (self.beat_sync_b, self.lock_b, self.beat_trigger_b, self.volume_peak_state_b),
        };
        if !synced || locked || trigger.mode != BeatTriggerMode::VolumePeak {
            return;
        }
        let result = detect_volume_peak(rms, state, trigger.sensitivity, now_ms);
        match deck {
            Deck::A => self.volume_peak_state_a = result.next,
            Deck::B => self.volume_peak_state_b = result.next,
        }
        if result.triggered {
            self.advance_or_navigate(deck);
        }
    }

    /// Reads back the current value of a snapshot-capturable `CommandId`:
    /// the inverse of the `CommandContext::set_*` setters those same ids
    /// dispatch to. `None` for any `CommandId` outside
    /// `SNAPSHOT_CAPTURABLE_IDS`, whether because it has no real setter yet
    /// or because it is deliberately excluded: the `CommandId::Time*` and
    /// `CommandId::Qvar*` families do have real setters (steps 8 and 9) and
    /// still return `None` here on purpose; see `SNAPSHOT_CAPTURABLE_IDS`
    /// for why.
    pub fn get_command_value(&self, id: CommandId) -> Option<f64> {
        match id {
            CommandId::ColorHueA => Some(self.color_params_a.hue_rotate),
            CommandId::ColorSatA => Some(self.color_params_a.saturate),
            CommandId::ColorBrightA => Some(self.color_params_a.brightness),
            CommandId::ColorContrastA => Some(self.color_params_a.contrast),
            CommandId::ColorInvertA => Some(self.color_params_a.invert),
            CommandId::ColorHueB => Some(self.color_params_b.hue_rotate),
            CommandId::ColorSatB => Some(self.color_params_b.saturate),
            CommandId::ColorBrightB => Some(self.color_params_b.brightness),
            CommandId::ColorContrastB => Some(self.color_params_b.contrast),
            CommandId::ColorInvertB => Some(self.color_params_b.invert),
            CommandId::CompositeBlend0 => Some(blend_mode_to_value01(self.slot_composites[0].blend)),
            CommandId::CompositeBlend1 => Some(blend_mode_to_value01(self.slot_composites[1].blend)),
            CommandId::CompositeBlend2 => Some(blend_mode_to_value01(self.slot_composites[2].blend)),
            CommandId::CompositeBlend3 => Some(blend_mode_to_value01(self.slot_composites[3].blend)),
            CommandId::LumakeyBlack0 => Some(self.slot_composites[0].luma_black),
            CommandId::LumakeyBlack1 => Some(self.slot_composites[1].luma_black),
            CommandId::LumakeyBlack2 => Some(self.slot_composites[2].luma_black),
            CommandId::LumakeyBlack3 => Some(self.slot_composites[3].luma_black),
            CommandId::LumakeyWhite0 => Some(self.slot_composites[0].luma_white),
            CommandId::LumakeyWhite1 => Some(self.slot_composites[1].luma_white),
            CommandId::LumakeyWhite2 => Some(self.slot_composites[2].luma_white),
            CommandId::LumakeyWhite3 => Some(self.slot_composites[3].luma_white),
            CommandId::ColorkeyHue0 => Some(self.slot_composites[0].color_hue),
            CommandId::ColorkeyHue1 => Some(self.slot_composites[1].color_hue),
            CommandId::ColorkeyHue2 => Some(self.slot_composites[2].color_hue),
            CommandId::ColorkeyHue3 => Some(self.slot_composites[3].color_hue),
            CommandId::ColorkeyTolerance0 => Some(self.slot_composites[0].color_tol),
            CommandId::ColorkeyTolerance1 => Some(self.slot_composites[1].color_tol),
            CommandId::ColorkeyTolerance2 => Some(self.slot_composites[2].color_tol),
            CommandId::ColorkeyTolerance3 => Some(self.slot_composites[3].color_tol),
            _ => None,
        }
    }

    /// Captures the current value of every snapshot-capturable `CommandId`
    /// (`SNAPSHOT_CAPTURABLE_IDS`): what the snapshot panel's Save button
    /// stores into a slot.
    pub fn capture_snapshot_values(&self) -> HashMap<CommandId, f64> {
        SNAPSHOT_CAPTURABLE_IDS.iter().filter_map(|&id| self.get_command_value(id).map(|v| (id, v))).collect()
    }

    /// Advances an in-progress snapshot recall by `dt_sec` (same
    /// caller-supplied-dt convention as `tick_playlists`) and returns the
    /// interpolated `(CommandId, value)` pairs for this tick. Dispatching
    /// them is the caller's job (`app::about_to_wait`, through
    /// `CommandRegistry::dispatch`) so a recall stays on the same
    /// keyboard/MIDI/OSC/remote-ws parity path as every other command.
    /// Returns an empty `Vec` when no recall is active, or when the target
    /// slot was cleared mid-recall (which also clears `active_recall`).
    pub fn tick_recall(&mut self, dt_sec: f64) -> Vec<(CommandId, f64)> {
        let Some(active) = &self.active_recall else { return Vec::new() };
        let slot = active.slot;
        let Some(snapshot) = &self.snapshot_slots[slot] else {
            self.active_recall = None;
            return Vec::new();
        };
        let (values, next) = tick_active_recall(active, &snapshot.values, self.snapshot_recall_duration_sec, dt_sec);
        self.active_recall = next;
        values.into_iter().collect()
    }

    /// Advances timeline playback by `dt_sec` (same caller-supplied-dt
    /// convention as `tick_recall`/`tick_playlists` above) and returns the
    /// interpolated `(CommandId, value)` pairs for this tick: dispatching
    /// them is the caller's job (`app::about_to_wait`, through
    /// `CommandRegistry::dispatch`), same keyboard/MIDI/OSC/remote-ws
    /// parity reasoning as `tick_recall`. Returns an empty `Vec` when not
    /// playing, or when there are fewer than 2 keyframes (nothing to loop
    /// over: `timeline_loop_duration` is 0 in that case).
    pub fn tick_timeline(&mut self, dt_sec: f64) -> Vec<(CommandId, f64)> {
        if !self.timeline_playing {
            return Vec::new();
        }
        let duration = timeline_loop_duration(&self.timeline_keyframes);
        if duration <= 0.0 {
            return Vec::new();
        }
        self.timeline_elapsed_sec += dt_sec;
        let t = self.timeline_elapsed_sec % duration;
        timeline_values_at(&self.timeline_keyframes, &self.snapshot_slots, t).into_iter().collect()
    }
}

/// `CommandId`s the snapshot panel's Save button captures and a recall
/// interpolates toward: the subset of commands currently addressable by a
/// real `CommandContext` setter (Color/Composite, steps 1-2 of the Phase 8
/// plan). Grows as later steps add their own setters; Keymap (step 3) added
/// none. See `Show::get_command_value`.
///
/// **The 32 `CommandId::Time*` setters (step 8) are deliberately left out**,
/// rather than overlooked. Two reasons, both worth revisiting once the LFO
/// step lands and the trade-off is easier to judge:
///
/// - *Bandwidth.* Time values reach the decks through a side channel that
///   carries one value per deck per frame (see `engine::preset_patch`). A
///   recall interpolates every captured id every frame, so adding Time would
///   put up to 7 continuously-changing values per deck on a 1-per-frame
///   channel: each would land at roughly 1/7 the recall's own rate, making a
///   recall visibly step rather than glide, and starving whatever else was
///   already moving. Every id currently in this list is applied host-side by
///   the GPU compositor, with no such ceiling.
/// - *Round-tripping the scale.* Everything here is stored 0..1, which is
///   also the `value01` a recall dispatches. Time is stored 0..2 (see
///   `time_params::TIME_MULT_MAX`), so `get_command_value` would have to
///   divide by `TIME_MULT_MAX` to invert `commands::time_mult` exactly:
///   easy to get wrong, and silently wrong if it ever is, since a snapshot
///   would recall to half or double what was saved.
///
/// **The 128 `CommandId::Qvar*` setters (step 9) are left out for the same
/// two reasons**, both of which apply at least as strongly: they share the
/// very same one-value-per-deck-per-frame channel (so a captured Qvar would
/// compete with Time *and* with the other 31 watches), and they are stored
/// -2..2 rather than 0..1, so the same inversion of `commands::q_var_value`
/// would have to be exact. A third reason is specific to Qvar: dispatching
/// one *enables* the watch it addresses, so a recall would silently switch
/// watches on, and switching a watch on re-patches and reloads the deck's
/// preset: a recall that did that every frame would be a reload loop.
///
/// Consequence to be aware of: saving a snapshot captures neither Time nor
/// Qvar, and recalling one leaves both where they are.
const SNAPSHOT_CAPTURABLE_IDS: [CommandId; 30] = [
    CommandId::ColorHueA,
    CommandId::ColorSatA,
    CommandId::ColorBrightA,
    CommandId::ColorContrastA,
    CommandId::ColorInvertA,
    CommandId::ColorHueB,
    CommandId::ColorSatB,
    CommandId::ColorBrightB,
    CommandId::ColorContrastB,
    CommandId::ColorInvertB,
    CommandId::CompositeBlend0,
    CommandId::CompositeBlend1,
    CommandId::CompositeBlend2,
    CommandId::CompositeBlend3,
    CommandId::LumakeyBlack0,
    CommandId::LumakeyBlack1,
    CommandId::LumakeyBlack2,
    CommandId::LumakeyBlack3,
    CommandId::LumakeyWhite0,
    CommandId::LumakeyWhite1,
    CommandId::LumakeyWhite2,
    CommandId::LumakeyWhite3,
    CommandId::ColorkeyHue0,
    CommandId::ColorkeyHue1,
    CommandId::ColorkeyHue2,
    CommandId::ColorkeyHue3,
    CommandId::ColorkeyTolerance0,
    CommandId::ColorkeyTolerance1,
    CommandId::ColorkeyTolerance2,
    CommandId::ColorkeyTolerance3,
];

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
        // Whole-branch review Finding 19: keep `selected_slot` (what the
        // Decks panel highlights) in sync on a keyboard-driven switch too.
        // `select_slot` already derives `active_deck` from a clicked
        // slot's bus; without this, the reverse direction let the
        // highlighted card and the deck `PresetNextActive`/
        // `PlaylistNextActive` actually act on disagree. A deck whose bus
        // isn't assigned to any slot (both `Off`, or both slots on the
        // other letter) leaves `selected_slot` where it was, same
        // "no visible effect" contract `take_fired_presets` already has
        // for that case.
        if let Some(slot) = self.deck_bus_slot_for(self.active_deck) {
            self.selected_slot = slot;
        }
    }

    /// Cycles the deck's index through the full preset catalog (not the
    /// playlist: see `playlist::PlaylistEngine`) and reports the chosen
    /// name via `fired_preset_a`/`fired_preset_b`, drained by
    /// `take_fired_presets`. Port of `navigatePreset` in
    /// `+page.svelte:434-448`.
    fn navigate_preset(&mut self, deck: Deck, direction: i32) {
        if self.preset_catalog.is_empty() {
            return;
        }
        let len = self.preset_catalog.len();
        let idx_ref = match deck {
            Deck::A => &mut self.preset_index_a,
            Deck::B => &mut self.preset_index_b,
        };
        // Port of +page.svelte:436-439/441-444: avoids negative modulo.
        *idx_ref = if direction == 1 {
            (*idx_ref + 1) % len
        } else {
            ((if *idx_ref == 0 { len } else { *idx_ref }) - 1) % len
        };
        let name = self.preset_catalog[*idx_ref].name.clone();
        let cell = match deck {
            Deck::A => &self.fired_preset_a,
            Deck::B => &self.fired_preset_b,
        };
        *cell.borrow_mut() = Some(name);
    }

    fn toggle_playlist(&mut self, deck: Deck) {
        self.playlists.toggle_playlist(deck);
    }

    fn playlist_next(&mut self, deck: Deck) {
        self.playlists.playlist_next(deck);
    }

    fn playlist_prev(&mut self, deck: Deck) {
        self.playlists.playlist_prev(deck);
    }

    fn get_playlist_playing(&self, deck: Deck) -> bool {
        match deck {
            Deck::A => self.playlists.a_playing,
            Deck::B => self.playlists.b_playing,
        }
    }

    // The overlay queue is Phase 4/M2+ territory (see commands.rs's own
    // header note: most CommandId variants are no-op stubs in the TS
    // source too, wired up by later milestones).
    fn advance_overlay_queue(&mut self, _direction: i32) {}

    fn set_color_hue_a(&mut self, v: f64) {
        self.color_params_a.hue_rotate = v.clamp(0.0, 1.0);
    }

    fn set_color_sat_a(&mut self, v: f64) {
        self.color_params_a.saturate = v.clamp(0.0, 1.0);
    }

    fn set_color_bright_a(&mut self, v: f64) {
        self.color_params_a.brightness = v.clamp(0.0, 1.0);
    }

    fn set_color_contrast_a(&mut self, v: f64) {
        self.color_params_a.contrast = v.clamp(0.0, 1.0);
    }

    fn set_color_invert_a(&mut self, v: f64) {
        self.color_params_a.invert = v.clamp(0.0, 1.0);
    }

    fn set_color_hue_b(&mut self, v: f64) {
        self.color_params_b.hue_rotate = v.clamp(0.0, 1.0);
    }

    fn set_color_sat_b(&mut self, v: f64) {
        self.color_params_b.saturate = v.clamp(0.0, 1.0);
    }

    fn set_color_bright_b(&mut self, v: f64) {
        self.color_params_b.brightness = v.clamp(0.0, 1.0);
    }

    fn set_color_contrast_b(&mut self, v: f64) {
        self.color_params_b.contrast = v.clamp(0.0, 1.0);
    }

    fn set_color_invert_b(&mut self, v: f64) {
        self.color_params_b.invert = v.clamp(0.0, 1.0);
    }

    fn set_time_speed(&mut self, slot: usize, v: f64) {
        self.time_params[slot].speed_mult = clamp_time_mult(v);
    }

    fn set_time_zoom(&mut self, slot: usize, v: f64) {
        self.time_params[slot].zoom_mult = clamp_time_mult(v);
    }

    fn set_time_rot(&mut self, slot: usize, v: f64) {
        self.time_params[slot].rot_mult = clamp_time_mult(v);
    }

    fn set_time_warp(&mut self, slot: usize, v: f64) {
        self.time_params[slot].warp_mult = clamp_time_mult(v);
    }

    fn set_time_dx(&mut self, slot: usize, v: f64) {
        self.time_params[slot].dx_mult = clamp_time_mult(v);
    }

    fn set_time_dy(&mut self, slot: usize, v: f64) {
        self.time_params[slot].dy_mult = clamp_time_mult(v);
    }

    fn set_time_stretch(&mut self, slot: usize, v: f64) {
        self.time_params[slot].stretch_mult = clamp_time_mult(v);
    }

    fn set_time_wave(&mut self, slot: usize, v: f64) {
        self.time_params[slot].wave_mult = clamp_time_mult(v);
    }

    /// Built from `q_vars`'s two ported helpers rather than indexing the
    /// arrays directly: they already carry the out-of-range guard the TS
    /// port grew (`slot > 3`, `n` outside 1..=32 is a no-op, not a panic),
    /// which matters here because `n`/`slot` come from a `CommandId` table
    /// and an OSC/remote-ws payload rather than from this crate. Order is
    /// load-bearing: `with_q_var_watch` resets the value to 0, so enabling
    /// has to happen *before* the value is written, not after.
    fn set_q_var(&mut self, slot: usize, n: usize, v: f64) {
        let enabled = with_q_var_watch(self.q_var_params, slot, n);
        self.q_var_params = with_q_var_value(enabled, slot, n, clamp_q_var_value(v));
    }

    fn set_composite_blend(&mut self, slot: usize, v: f64) {
        self.slot_composites[slot].blend = blend_mode_from_value01(v);
    }

    fn set_composite_luma_black(&mut self, slot: usize, v: f64) {
        self.slot_composites[slot].luma_black = v.clamp(0.0, 1.0);
    }

    fn set_composite_luma_white(&mut self, slot: usize, v: f64) {
        self.slot_composites[slot].luma_white = v.clamp(0.0, 1.0);
    }

    fn set_composite_color_hue(&mut self, slot: usize, v: f64) {
        self.slot_composites[slot].color_hue = v.clamp(0.0, 1.0);
    }

    fn set_composite_color_tol(&mut self, slot: usize, v: f64) {
        self.slot_composites[slot].color_tol = v.clamp(0.0, 1.0);
    }

    fn recall_snapshot(&mut self, slot: usize) {
        let Some(snapshot) = &self.snapshot_slots[slot] else { return };
        let start_values: HashMap<CommandId, f64> =
            snapshot.values.keys().filter_map(|&id| self.get_command_value(id).map(|v| (id, v))).collect();
        self.active_recall = Some(ActiveRecall { slot, start_values, elapsed_sec: 0.0 });
    }

    fn toggle_timeline(&mut self) {
        self.timeline_playing = !self.timeline_playing;
        if self.timeline_playing {
            self.timeline_elapsed_sec = 0.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blend::BlendMode;
    use crate::commands::{create_default_registry, CommandId};
    use crate::snapshot::smoothstep;

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
            // Whole-branch review Finding 19: a keyboard-driven active-deck
            // switch used to leave `selected_slot` (what the Decks panel
            // highlights) pointing at the previous deck's slot.
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

    mod beat_sync_defaults {
        use super::*;

        #[test]
        fn beat_sync_starts_false_for_both_decks() {
            let show = Show::default();
            assert!(!show.beat_sync_a);
            assert!(!show.beat_sync_b);
        }

        #[test]
        fn beat_trigger_configs_start_at_default() {
            let show = Show::default();
            assert_eq!(show.beat_trigger_a, default_beat_trigger_config());
            assert_eq!(show.beat_trigger_b, default_beat_trigger_config());
        }

        #[test]
        fn auto_xfade_starts_false_with_beats_per_change_8() {
            let show = Show::default();
            assert!(!show.auto_xfade);
            assert_eq!(show.beats_per_change, 8);
        }

        #[test]
        fn manual_bpm_starts_at_zero() {
            assert_eq!(Show::default().manual_bpm, 0.0);
        }
    }

    mod on_beat {
        use super::*;

        fn catalog(names: &[&str]) -> Vec<PresetMeta> {
            names.iter().map(|n| PresetMeta { name: n.to_string(), category: "Other".to_string() }).collect()
        }

        #[test]
        fn auto_xfade_does_not_toggle_before_the_configured_beat_count() {
            let mut show = Show { auto_xfade: true, beats_per_change: 4, ..Default::default() };
            for _ in 0..3 {
                show.on_beat();
                assert_eq!(show.crossfader, 0.0);
            }
        }

        #[test]
        fn auto_xfade_toggles_on_the_nth_beat() {
            let mut show = Show { auto_xfade: true, beats_per_change: 4, ..Default::default() };
            for _ in 0..4 {
                show.on_beat();
            }
            assert_eq!(show.crossfader, 1.0);
        }

        #[test]
        fn auto_xfade_toggles_back_after_another_n_beats() {
            let mut show = Show { auto_xfade: true, beats_per_change: 2, ..Default::default() };
            show.on_beat();
            show.on_beat();
            assert_eq!(show.crossfader, 1.0);
            show.on_beat();
            show.on_beat();
            assert_eq!(show.crossfader, 0.0);
        }

        #[test]
        fn reset_auto_xfade_count_restarts_the_cadence_from_the_top() {
            // Whole-branch review Finding 7: without a reset, re-enabling
            // auto-xfade mid-count would fire the next crossfade early.
            let mut show = Show { auto_xfade: true, beats_per_change: 4, ..Default::default() };
            show.on_beat();
            show.on_beat();
            show.on_beat(); // 3 beats in, one short of the 4-beat cadence
            assert_eq!(show.crossfader, 0.0);
            show.reset_auto_xfade_count();
            show.on_beat(); // would have flipped here without the reset
            assert_eq!(show.crossfader, 0.0);
            show.on_beat();
            show.on_beat();
            show.on_beat();
            assert_eq!(show.crossfader, 1.0); // flips a full 4 beats after the reset
        }

        #[test]
        fn does_not_touch_crossfader_when_auto_xfade_is_off() {
            let mut show = Show { beats_per_change: 1, ..Default::default() };
            show.on_beat();
            assert_eq!(show.crossfader, 0.0);
        }

        #[test]
        fn does_not_advance_when_beat_sync_is_false() {
            let mut show = Show { preset_catalog: catalog(&["P0", "P1"]), ..Default::default() };
            // beat_sync_a/b default to false.
            show.on_beat();
            assert!(show.fired_preset_a.borrow().is_none());
            assert!(show.fired_preset_b.borrow().is_none());
        }

        #[test]
        fn does_not_advance_when_locked_even_if_beat_sync_is_true() {
            let mut show = Show { preset_catalog: catalog(&["P0", "P1"]), beat_sync_a: true, lock_a: true, ..Default::default() };
            show.on_beat();
            assert!(show.fired_preset_a.borrow().is_none());
        }

        #[test]
        fn advances_the_playlist_when_it_has_items() {
            let mut show = Show::default();
            show.playlists.add_to_playlist(Deck::A, "p1".to_string());
            show.playlists.add_to_playlist(Deck::A, "p2".to_string());
            show.beat_sync_a = true;
            show.on_beat();
            assert_eq!(show.playlists.engine_a_mut().unwrap().current_index(), 1);
        }

        #[test]
        fn navigates_the_preset_catalog_when_the_playlist_is_empty() {
            let mut show = Show { preset_catalog: catalog(&["P0", "P1"]), beat_sync_a: true, ..Default::default() };
            show.on_beat();
            assert_eq!(show.fired_preset_a.borrow().as_deref(), Some("P1"));
        }

        #[test]
        fn deck_b_advances_independently_of_deck_a() {
            let mut show = Show::default();
            show.playlists.add_to_playlist(Deck::B, "p1".to_string());
            show.playlists.add_to_playlist(Deck::B, "p2".to_string());
            show.beat_sync_b = true;
            show.on_beat();
            assert_eq!(show.playlists.engine_b_mut().unwrap().current_index(), 1);
            assert!(show.fired_preset_a.borrow().is_none());
        }

        #[test]
        fn respects_the_configured_trigger_cadence_via_the_clock_beat_count() {
            let mut show = Show { preset_catalog: catalog(&["P0", "P1"]), beat_sync_a: true, ..Default::default() };
            show.beat_trigger_a.beats_per_change = 4;
            show.clock.pulse(None); // beat_count = 1
            show.on_beat();
            show.clock.pulse(None); // 2
            show.on_beat();
            show.clock.pulse(None); // 3
            show.on_beat();
            assert!(show.fired_preset_a.borrow().is_none());
            show.clock.pulse(None); // 4
            show.on_beat();
            assert_eq!(show.fired_preset_a.borrow().as_deref(), Some("P1"));
        }
    }

    mod toggle_beat_sync {
        use super::*;

        #[test]
        fn sets_the_playlist_interval_to_infinity_then_restores_it() {
            let mut show = Show::default();
            show.playlists.add_to_playlist(Deck::A, "p1".to_string());
            show.playlists.interval_sec = 5.0;

            show.toggle_beat_sync(Deck::A);
            assert!(show.beat_sync_a);
            assert_eq!(show.playlists.engine_a_mut().unwrap().interval_ms(), f64::INFINITY);

            show.toggle_beat_sync(Deck::A);
            assert!(!show.beat_sync_a);
            assert_eq!(show.playlists.engine_a_mut().unwrap().interval_ms(), 5000.0);
        }
    }

    mod tap_tempo {
        use super::*;

        #[test]
        fn a_single_tap_does_not_set_a_bpm() {
            let mut show = Show::default();
            show.tap_tempo(0.0);
            assert_eq!(show.manual_bpm, 0.0);
        }

        #[test]
        fn computes_bpm_from_a_regular_interval() {
            let mut show = Show::default();
            show.tap_tempo(0.0);
            show.tap_tempo(500.0); // 500ms -> 120bpm
            assert_eq!(show.manual_bpm, 120.0);
            assert_eq!(show.clock.bpm(), 120.0);
        }

        #[test]
        fn averages_over_all_kept_intervals() {
            let mut show = Show::default();
            show.tap_tempo(0.0);
            show.tap_tempo(400.0); // interval 400
            show.tap_tempo(1000.0); // interval 600, avg 500 -> 120bpm
            assert_eq!(show.manual_bpm, 120.0);
        }

        #[test]
        fn rejects_a_computed_bpm_below_40() {
            let mut show = Show::default();
            show.tap_tempo(0.0);
            show.tap_tempo(3000.0); // 3000ms -> 20bpm
            assert_eq!(show.manual_bpm, 0.0);
        }

        #[test]
        fn rejects_a_computed_bpm_above_300() {
            let mut show = Show::default();
            show.tap_tempo(0.0);
            show.tap_tempo(100.0); // 100ms -> 600bpm
            assert_eq!(show.manual_bpm, 0.0);
        }

        #[test]
        fn keeps_only_the_last_4_taps() {
            let mut show = Show::default();
            show.tap_tempo(0.0);
            show.tap_tempo(500.0);
            show.tap_tempo(1000.0);
            show.tap_tempo(1500.0);
            show.tap_tempo(2000.0); // 5th tap: oldest (0.0) is dropped
            assert_eq!(show.tap_times, vec![500.0, 1000.0, 1500.0, 2000.0]);
        }
    }

    mod clear_manual_bpm {
        use super::*;

        #[test]
        fn resets_manual_bpm_tap_times_and_clock_bpm() {
            let mut show = Show::default();
            show.tap_tempo(0.0);
            show.tap_tempo(500.0);
            assert_eq!(show.manual_bpm, 120.0);

            show.clear_manual_bpm();
            assert_eq!(show.manual_bpm, 0.0);
            assert!(show.tap_times.is_empty());
            assert_eq!(show.clock.bpm(), 0.0);
        }
    }

    mod current_bpm {
        use super::*;

        fn commit_detected_bpm(show: &mut Show) {
            for i in 0..43 {
                show.beat_detector.process_sample(10.0, i as f64 * 10.0);
            }
            let mut now = 10_000.0;
            show.beat_detector.process_sample(50.0, now);
            now += 310.0;
            show.beat_detector.process_sample(50.0, now);
            now += 310.0;
            show.beat_detector.process_sample(50.0, now); // commits bpm = 194.0
        }

        #[test]
        fn returns_zero_when_neither_manual_nor_detected_bpm_is_set() {
            assert_eq!(Show::default().current_bpm(), 0.0);
        }

        #[test]
        fn falls_back_to_the_detected_bpm_when_no_manual_bpm_is_set() {
            let mut show = Show::default();
            commit_detected_bpm(&mut show);
            assert_eq!(show.current_bpm(), 194.0);
        }

        #[test]
        fn manual_bpm_takes_priority_over_the_detected_bpm() {
            let mut show = Show::default();
            commit_detected_bpm(&mut show);
            show.manual_bpm = 128.0;
            assert_eq!(show.current_bpm(), 128.0);
        }
    }

    mod check_volume_peak_triggers {
        use super::*;

        fn catalog(names: &[&str]) -> Vec<PresetMeta> {
            names.iter().map(|n| PresetMeta { name: n.to_string(), category: "Other".to_string() }).collect()
        }

        fn volume_peak_trigger() -> BeatTriggerConfig {
            BeatTriggerConfig { mode: BeatTriggerMode::VolumePeak, beats_per_change: 8, offset: 0, sensitivity: 0.5 }
        }

        #[test]
        fn does_nothing_in_beat_mode() {
            let mut show = Show { preset_catalog: catalog(&["P0", "P1"]), beat_sync_a: true, ..Default::default() }; // default trigger mode is Beat
            show.check_volume_peak_triggers(0.9, 1000.0);
            assert!(show.fired_preset_a.borrow().is_none());
        }

        #[test]
        fn does_nothing_when_not_synced() {
            let mut show = Show { preset_catalog: catalog(&["P0", "P1"]), beat_trigger_a: volume_peak_trigger(), ..Default::default() };
            // beat_sync_a defaults to false.
            show.check_volume_peak_triggers(0.9, 1000.0);
            assert!(show.fired_preset_a.borrow().is_none());
        }

        #[test]
        fn does_nothing_when_locked() {
            let mut show = Show { preset_catalog: catalog(&["P0", "P1"]), beat_trigger_a: volume_peak_trigger(), beat_sync_a: true, lock_a: true, ..Default::default() };
            show.check_volume_peak_triggers(0.9, 1000.0);
            assert!(show.fired_preset_a.borrow().is_none());
        }

        #[test]
        fn advances_on_a_clear_peak_in_volume_peak_mode() {
            let mut show = Show { preset_catalog: catalog(&["P0", "P1"]), beat_trigger_a: volume_peak_trigger(), beat_sync_a: true, ..Default::default() };
            show.check_volume_peak_triggers(0.2, 1000.0); // establishes the rolling average
            show.check_volume_peak_triggers(0.9, 1600.0); // clear peak above it
            assert_eq!(show.fired_preset_a.borrow().as_deref(), Some("P1"));
        }

        #[test]
        fn processes_both_decks_independently() {
            let mut show = Show { preset_catalog: catalog(&["P0", "P1"]), beat_trigger_a: volume_peak_trigger(), beat_trigger_b: volume_peak_trigger(), beat_sync_a: true, beat_sync_b: true, ..Default::default() };
            show.check_volume_peak_triggers(0.2, 1000.0);
            show.check_volume_peak_triggers(0.9, 1600.0);
            assert_eq!(show.fired_preset_a.borrow().as_deref(), Some("P1"));
            assert_eq!(show.fired_preset_b.borrow().as_deref(), Some("P1"));
        }
    }

    mod reseed_rng {
        use super::*;

        #[test]
        fn makes_deck_a_shuffle_draws_actually_differ_across_seeds() {
            let draws = |seed: u64| -> Vec<usize> {
                let mut show = Show::default();
                for name in ["p1", "p2", "p3", "p4", "p5"] {
                    show.playlists.add_to_playlist(Deck::A, name.to_string());
                }
                show.playlists.engine_a_mut().unwrap().set_mode(PlaylistMode::Shuffle);
                show.reseed_rng(seed);
                (0..10)
                    .map(|_| {
                        show.playlist_next(Deck::A);
                        show.playlists.engine_a_mut().unwrap().current_index()
                    })
                    .collect()
            };
            assert_ne!(draws(1), draws(2));
        }

        #[test]
        fn seeds_deck_a_and_deck_b_differently_so_they_do_not_lockstep() {
            let mut show = Show::default();
            for name in ["p1", "p2", "p3", "p4", "p5"] {
                show.playlists.add_to_playlist(Deck::A, name.to_string());
                show.playlists.add_to_playlist(Deck::B, name.to_string());
            }
            show.playlists.engine_a_mut().unwrap().set_mode(PlaylistMode::Shuffle);
            show.playlists.engine_b_mut().unwrap().set_mode(PlaylistMode::Shuffle);
            show.reseed_rng(42);
            let draws_a: Vec<usize> = (0..10)
                .map(|_| {
                    show.playlist_next(Deck::A);
                    show.playlists.engine_a_mut().unwrap().current_index()
                })
                .collect();
            let draws_b: Vec<usize> = (0..10)
                .map(|_| {
                    show.playlist_next(Deck::B);
                    show.playlists.engine_b_mut().unwrap().current_index()
                })
                .collect();
            assert_ne!(draws_a, draws_b);
        }
    }
}
