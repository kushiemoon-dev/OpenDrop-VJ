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
use crate::lfo::LfoEngine;
use crate::overlay::OverlayStore;
use crate::playlist::{PlaylistEngine, PlaylistMode, PlaylistStore};
use crate::preset_index::PresetMeta;
use crate::q_vars::{clamp_q_var_value, default_q_var_params, with_q_var_value, with_q_var_watch, QVarParamsTuple};
use crate::rng::Xorshift64;
use crate::snapshot::{tick_active_recall, ActiveRecall, Snapshot};
use crate::strobe::StrobeState;
use crate::time_params::{clamp_time_mult, DeckTimeParams};
use crate::timeline::{timeline_loop_duration, timeline_values_at, TimelineKeyframe};
use crate::video::VideoState;

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
    /// The 8 Time multipliers per deck slot, written by the Time panel and
    /// by the 32 `CommandId::Time*` setters.
    /// Read by `app`'s per-frame push loop, which forwards the changed ones
    /// into each deck's running preset: `Show` itself stays engine-free.
    pub time_params: [DeckTimeParams; 4],
    /// The 32 q-var overrides per deck slot, written by the Qvar panel and
    /// by the 128 `CommandId::Qvar*` setters.
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
    /// Set while the energy auto-pilot picks the presets of that bus. It then
    /// replaces the automatic advance: beat sync, volume peak triggers and the
    /// playlist timer leave the bus alone. Manual navigation is unaffected.
    pub autopilot_a: bool,
    pub autopilot_b: bool,
    pub clock: Clock,
    pub beat_detector: BeatDetector,
    pub beat_sync_a: bool,
    pub beat_sync_b: bool,
    pub beat_trigger_a: BeatTriggerConfig,
    pub beat_trigger_b: BeatTriggerConfig,
    /// The 8 snapshot slots. `None` = empty
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
    /// Up to 8 keyframes sequencing playback
    /// across the existing snapshot slots. Kept sorted by `time_sec` by
    /// the timeline panel (`app::ui::timeline`): `timeline_values_at`
    /// assumes sorted input.
    pub timeline_keyframes: Vec<TimelineKeyframe>,
    /// Whether the timeline loop is currently advancing. Toggled through
    /// `CommandContext::toggle_timeline` (keyboard/MIDI/OSC/remote-ws
    /// parity), advanced each render tick by `tick_timeline`.
    pub timeline_playing: bool,
    /// Seconds elapsed since timeline playback last started, accumulated
    /// by `tick_timeline` from caller-supplied dt, same "no wall clock of
    /// its own" convention as `tick_recall` (see that method's doc
    /// comment). Reset to 0 by `toggle_timeline` on every transition to
    /// `true`, so play always restarts at the beginning of the current
    /// loop cycle rather than resuming stale progress or jumping in time.
    timeline_elapsed_sec: f64,
    /// Strobe on/off/rate/intensity/color.
    /// Toggled through `CommandContext::toggle_strobe` (keyboard/MIDI/OSC/
    /// remote-ws parity); rate/intensity/color are written directly by
    /// the panel (`app::ui::strobe`), same "direct field mutation"
    /// convention as `slot_composites`/`color_params_a`. The per-frame
    /// flash intensity itself is computed by `app::about_to_wait` via
    /// `strobe::strobe_flash_intensity(&show.strobe, ...)` and handed to
    /// the compositor: `Show` stores only the user-facing state, no GL.
    pub strobe: StrobeState,
    /// 4 modulation slots, written directly
    /// by the LFO panel (`app::ui::lfo`), same "direct field mutation"
    /// convention as `strobe`'s rate/intensity/color above. Ticked each
    /// frame by `app::about_to_wait` (`LfoEngine::tick`, driven by
    /// `clock.phase01()`); the resulting `(CommandId, value01)` pairs are
    /// dispatched through the registry, not written to `Show` here, same
    /// registry-dispatch parity as `tick_recall`/`tick_timeline` above, so
    /// an LFO route moves its target through the exact same
    /// keyboard/MIDI/OSC/remote-ws-equivalent path.
    pub lfo_engine: LfoEngine,
    /// Sprite/text overlays and their auto-cycling queue. `core::overlay`
    /// was ported long before it had a
    /// consumer; this field is what finally gives it one. Written directly
    /// by the Overlays panel (`app::ui::overlays`), same "direct field
    /// mutation" convention as `strobe`/`lfo_engine` above, except the
    /// queue's ◀/▶ buttons, which go through
    /// `CommandContext::advance_overlay_queue` for keyboard/MIDI/OSC/
    /// remote-ws parity. The queue also self-advances: `on_beat` and
    /// `check_volume_peak_triggers` below feed it the same two signals the
    /// per-deck playlist advance already runs on.
    ///
    /// No GL and no asset paths here: `Overlay` carries only the values a
    /// zero-I/O crate can own, and `app` keeps the id→file map and the
    /// id→GL-texture cache alongside it (see `AppState::overlay_assets`).
    pub overlay_store: OverlayStore,
    /// Video background layer: on/off, opacity, clip rotation, and the 4
    /// beat/volume-reactive toggles. Written
    /// directly by the Video panel (`app::ui::video`), same "direct field
    /// mutation" convention as `strobe`/`lfo_engine`/`overlay_store` above.
    ///
    /// Same boundary as `overlay_store`: no clip file paths, no decoder
    /// handle, no GL here. `app` owns the clip library (id → file), the
    /// `ffmpeg` capture thread (`opendrop_io::video_capture`), and the GL
    /// texture the decoded frames land in; this field holds only the values
    /// a zero-I/O crate can own. The beat-driven clip cut is
    /// [`Show::on_video_beat`], called by `app` (not from `on_beat` itself)
    /// because it needs that app-owned clip list.
    pub video: VideoState,
    /// Shuffle-mode draws for `on_video_beat`: a 5th independent
    /// `Xorshift64` consumer alongside the two playlist engines, the LFO,
    /// and the overlay queue. See `reseed_rng`.
    video_rng: Xorshift64,
    /// One `VideoState` per deck slot, independent of the global `video`
    /// layer above: each slot cuts on its own beat via
    /// [`Show::on_deck_video_beat`].
    pub deck_video: [VideoState; 4],
    /// One more independent `Xorshift64` consumer per deck slot, alongside
    /// the 5 already listed on `video_rng`'s own doc comment. See
    /// `reseed_rng`.
    deck_video_rng: [Xorshift64; 4],
    /// Which rkbx_link DJ deck index (0-3) is mapped to each of the 4 visual
    /// deck slots, or `None` (the default: nothing happens for an unmapped DJ
    /// deck). Indexed by DJ deck, not slot: `rkbx_deck_mapping[dj_deck] ==
    /// Some(slot)`.
    pub rkbx_deck_mapping: [Option<usize>; 4],
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
            autopilot_a: false,
            autopilot_b: false,
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
            strobe: StrobeState::default(),
            lfo_engine: LfoEngine::new(),
            overlay_store: OverlayStore::new(),
            video: VideoState::default(),
            video_rng: Xorshift64::default(),
            deck_video: std::array::from_fn(|_| VideoState::default()),
            deck_video_rng: [Xorshift64::default(); 4],
            rkbx_deck_mapping: [None; 4],
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
    /// per-launch entropy supplied by the caller (`app`'s bootstrap;
    /// `core` stays zero-I/O and has no clock of its own). Deck A and B get
    /// distinct-but-derived seeds so they don't draw identical shuffle
    /// sequences from the same source entropy. Without this, shuffle mode
    /// replayed the exact same sequence every single app launch.
    pub fn reseed_rng(&mut self, seed: u64) {
        if let Some(engine) = self.playlists.engine_a_mut() {
            engine.reseed_rng(seed);
        }
        if let Some(engine) = self.playlists.engine_b_mut() {
            engine.reseed_rng(seed ^ 0xA5A5_A5A5_A5A5_A5A5);
        }
        // LFO Sample & Hold, an unrelated RNG consumer, no lockstep risk
        // with the playlist engines above, so it reseeds from the raw
        // `seed` rather than an XOR'd variant.
        self.lfo_engine.reseed_rng(seed);
        // Overlay auto-cycle queue in shuffle mode, a third independent
        // consumer, XOR'd like deck B's so two shuffles driven by the same
        // beat don't advance in lockstep.
        self.overlay_store.reseed_rng(seed ^ 0x5A5A_5A5A_5A5A_5A5A);
        // Video-loop shuffle, a fifth independent consumer, XOR'd with its
        // own constant for the same anti-lockstep reason as the two above:
        // the overlay queue and the video layer are both driven by the
        // same beat, and two shuffles drawing identical sequences would
        // be visible.
        self.video_rng.reseed(seed ^ 0x3C3C_3C3C_3C3C_3C3C);
        // Per-deck video shuffle: 4 more independent consumers, each XOR'd
        // with its own constant for the same anti-lockstep reason as every
        // RNG reseed above.
        const DECK_VIDEO_RNG_XOR: [u64; 4] =
            [0x1E1E_1E1E_1E1E_1E1E, 0x2D2D_2D2D_2D2D_2D2D, 0x4B4B_4B4B_4B4B_4B4B, 0x6E6E_6E6E_6E6E_6E6E];
        for (slot, xor) in DECK_VIDEO_RNG_XOR.into_iter().enumerate() {
            self.deck_video_rng[slot].reseed(seed ^ xor);
        }
    }

    /// Beat-driven video-clip cut. Returns
    /// `true` when the current clip actually changed, so `app` knows to
    /// point its decoder at the new file.
    ///
    /// Not folded into [`Show::on_beat`] like the overlay queue's own
    /// advance, because the clip list it rotates through is `app`-owned
    /// (files on disk, see `Show::video`'s doc comment); `app` calls this
    /// immediately after `on_beat`, from the same two call sites.
    ///
    /// `ndi_active` is `NdiSnapshot::receive_active`: see
    /// `core::video`'s module doc comment for why that isn't mirrored into
    /// `VideoState`.
    pub fn on_video_beat(&mut self, all_clip_keys: &[String], ndi_active: bool) -> bool {
        let draw = self.video_rng.next_f64();
        self.video.on_beat(all_clip_keys, ndi_active, draw)
    }

    /// Beat-driven video-clip cut for deck slot `slot`. Same contract as
    /// [`Show::on_video_beat`], but per-slot: `slot`
    /// indexes both `deck_video` and `deck_video_rng`. `ndi_active` is always
    /// `false` here: a deck-video instance never exposes a live camera or NDI
    /// source (out of scope; see the Video panel's per-deck branch).
    pub fn on_deck_video_beat(&mut self, slot: usize, all_clip_keys: &[String]) -> bool {
        let draw = self.deck_video_rng[slot].next_f64();
        self.deck_video[slot].on_beat(all_clip_keys, false, draw)
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

    /// Maps DJ deck `dj_deck` to visual slot `visual_slot` (or unmaps it with
    /// `None`). Refuses (leaving the mapping untouched) if `visual_slot` is
    /// already claimed by a *different* DJ deck: a visual slot must never be
    /// driven by two DJ decks' sync at once.
    pub fn set_rkbx_deck_mapping(&mut self, dj_deck: usize, visual_slot: Option<usize>) -> Result<(), String> {
        if let Some(slot) = visual_slot {
            if let Some(other) = (0..self.rkbx_deck_mapping.len()).find(|&d| d != dj_deck && self.rkbx_deck_mapping[d] == Some(slot)) {
                return Err(format!("Deck {slot} is already mapped to DJ deck {other}"));
            }
        }
        self.rkbx_deck_mapping[dj_deck] = visual_slot;
        Ok(())
    }

    /// Whether automatic changes (auto-pilot, track recall) must leave `slot`
    /// alone because the deck bus it feeds is locked. An unassigned slot has no
    /// lock to respect.
    pub fn slot_locked(&self, slot: usize) -> bool {
        match self.deck_bus.get(slot) {
            Some(DeckBus::A) => self.lock_a,
            Some(DeckBus::B) => self.lock_b,
            _ => false,
        }
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
    /// preset is silently dropped, consistent with "Active" shortcuts
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

    /// Called by `app` once per beat emitted by `clock`/`beat_detector`.
    /// Port of `onBeat`, `beat-tempo-actions.ts:48-70` (minus the
    /// overlay/video/network pulse, out of scope here).
    pub fn on_beat(&mut self) {
        if self.auto_xfade {
            self.auto_xfade_count = (self.auto_xfade_count + 1) % self.beats_per_change.max(1);
            if self.auto_xfade_count == 0 {
                self.crossfader = if self.crossfader < 0.5 { 1.0 } else { 0.0 };
            }
        }
        self.maybe_advance_on_beat(Deck::A);
        self.maybe_advance_on_beat(Deck::B);
        // Overlay auto-cycle queue, the third `shouldTriggerOn
        // Beat` block of `beat-tempo-actions.ts`, alongside the two
        // per-deck ones just above. Unconditional here: the store's own
        // guard checks `queue_enabled` and the trigger mode.
        self.overlay_store.maybe_advance_on_beat(self.clock.beat_count() as i64);
        // Refreshes any LFO slot in `Sh` (sample & hold) shape: see
        // `LfoEngine::randomize_sh`'s doc comment ("call on each downbeat").
        self.lfo_engine.randomize_sh();
    }

    /// Restarts the auto-crossfade cadence from the top. Port of the
    /// unconditional `resetAutoXfadeCount()` call the TS reference makes on
    /// every auto-xfade toggle, either direction (`+page.svelte:1754`).
    /// `app` never called an equivalent of this: toggling auto-xfade off
    /// then back on resumed the crossfade cycle mid-count instead of
    /// restarting it, so the first crossfade after re-enabling could land
    /// 1-7 beats early/late.
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
    fn autopilot_on(&self, deck: Deck) -> bool {
        match deck {
            Deck::A => self.autopilot_a,
            Deck::B => self.autopilot_b,
        }
    }

    fn advance_or_navigate(&mut self, deck: Deck) {
        if self.autopilot_on(deck) {
            return;
        }
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
            let own_timer = !synced && !self.autopilot_on(deck);
            self.playlists.set_beat_sync_interval(deck, if own_timer { interval_ms } else { f64::INFINITY });
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
        self.clock.pulse(None); // resync phase only, bpm already set; does not emit a beat.
    }

    /// Port of `clearManualBpm`, `beat-tempo-actions.ts:113-117`.
    pub fn clear_manual_bpm(&mut self) {
        self.manual_bpm = 0.0;
        self.tap_times.clear();
        self.clock.set_bpm(0.0);
    }

    /// Manual BPM if set, otherwise the detected BPM (`0.0` if neither is
    /// available: the UI shows "-" on `0.0`, port of the ternary in
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
    /// supplied by `app`: see `audio::analysis::vu_level`.
    pub fn check_volume_peak_triggers(&mut self, rms: f64, now_ms: f64) {
        self.check_one_volume_peak(Deck::A, rms, now_ms);
        self.check_one_volume_peak(Deck::B, rms, now_ms);
        // Overlay auto-cycle queue, the volume-peak half of
        // what `on_beat` does for the beat half. Its own rolling
        // average/cooldown state lives on the store, not here (see
        // `OverlayStore::queue_volume_state`).
        self.overlay_store.maybe_advance_on_volume_peak(rms, now_ms);
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

mod command_context;
mod snapshots;

#[cfg(test)]
mod tests;
