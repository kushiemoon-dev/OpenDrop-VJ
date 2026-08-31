//! Live show state driving the compositor: crossfader, per-deck bus
//! assignment, per-slot composite config, per-bus color params. Pure
//! state/logic: no GL, no I/O. Implements `commands::CommandContext` so
//! the keyboard dispatch (`app::keymap` + `commands::create_default_registry`)
//! can drive it directly.
//!
//! `bus_gain` and the default bus assignment are ported from OpenDrop-VJ
//! `src/routes/+page.svelte:264-269`.

use std::cell::RefCell;
use std::rc::Rc;

use crate::beat_detector::BeatDetector;
use crate::beat_trigger::{
    default_beat_trigger_config, default_volume_peak_state, detect_volume_peak, should_trigger_on_beat,
    BeatTriggerConfig, BeatTriggerMode, VolumePeakState,
};
use crate::blend::{ColorParams, SlotComposite, DEFAULT_COLOR_PARAMS, DEFAULT_SLOT_COMPOSITE};
use crate::clock::Clock;
use crate::commands::{CommandContext, Deck};
use crate::playlist::{PlaylistEngine, PlaylistMode, PlaylistStore};
use crate::preset_index::PresetMeta;

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
