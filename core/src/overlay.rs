//! Port of OpenDrop-VJ `src/lib/engine/overlay.ts`,
//! `src/lib/engine/overlay-store.svelte.ts`, and the queue helpers from
//! `src/lib/engine/overlay-queue.ts` (imported by the store), merged into
//! one file: the `Overlay` value type plus the in-memory overlay list
//! management (CRUD, drag-over flag, auto-cycling queue) that wrapped it in
//! a Svelte store.
//!
//! Three adaptations for a zero-I/O, fully unit-testable `core`:
//! - `crypto.randomUUID()` needs OS entropy, which a zero-I/O crate has no
//!   access to. `make_overlay` and `OverlayStore::add_*` take the id as a
//!   parameter instead: the caller (a later, I/O-capable layer) generates
//!   it, same dependency-injection shape as `PlaylistEngine`'s `on_preset`
//!   callback in `playlist.rs`.
//! - `saveAsset`/`loadAsset`/`deleteAsset` (IndexedDB) and
//!   `addOverlayFromFile`/`onOverlayFilePick` (`FileReader`) are real
//!   storage/file I/O, not in-memory list management; dropped, they
//!   belong to a later I/O-capable phase. `add_overlay_at_position` and
//!   `remove_overlay` keep only the pure list-bookkeeping half of their TS
//!   counterparts.
//! - Queue shuffle used `Math.random()`. Same rationale as `playlist.rs`:
//!   a small deterministic xorshift64 PRNG instead, since no ported test
//!   asserts on actual randomness.
//!
//! Svelte `$state` reactivity is likewise dropped: `OverlayStore` fields
//! are plain struct fields mutated through `&mut self` methods.
//!
//! Wiring this module into `app` added the two pieces the original port
//! left on the other side of the I/O boundary but which are pure after
//! all:
//! - [`overlay_transform_at`] / [`parse_hex_color`]: the per-frame
//!   animation math `OverlayLayer.svelte` expressed as CSS animations,
//!   evaluated here instead so `engine::compositor` can draw the sprite.
//! - [`OverlayStore::maybe_advance_on_beat`] /
//!   [`OverlayStore::maybe_advance_on_volume_peak`]: the auto-cycle
//!   queue's two trigger paths (`beat-tempo-actions.ts:71-76` and
//!   `+page.svelte:558-562`), so `app`'s per-frame loop only has to pass
//!   on the beat/VU signals it already computes.

use std::collections::HashSet;

use crate::beat_trigger::{
    apply_beat_trigger_patch, default_beat_trigger_config, default_volume_peak_state, detect_volume_peak,
    should_trigger_on_beat, BeatTriggerConfig, BeatTriggerConfigPatch, BeatTriggerMode, VolumePeakState,
};
use crate::playlist::PlaylistMode;
use crate::rng::Xorshift64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayKind {
    Media,
    Text,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontFamily {
    Sans,
    Serif,
    Mono,
    Impact,
    Comic,
}

/// `Clone`: the Share panel builds an owned
/// `SharedSet` snapshot of the currently shareable overlays
/// (`share_set::filter_shareable_overlays` returns borrowed `&Overlay`s,
/// but `SharedSet::overlays: Vec<Overlay>` needs owned values); no other
/// consumer needed this before now.
#[derive(Debug, Clone)]
pub struct Overlay {
    pub id: String,
    pub name: String,
    /// normalized center X, 0-1
    pub x: f64,
    /// normalized center Y, 0-1
    pub y: f64,
    /// 1 = original size
    pub scale: f64,
    /// degrees
    pub rotation: f64,
    /// 0-1
    pub opacity: f64,
    /// CSS mix-blend-mode
    pub blend_mode: String,
    pub beat_reactive: bool,
    /// scale multiplier applied on the beat (e.g. 1.2)
    pub beat_scale: f64,
    /// true = video asset (rendered as <video> instead of <img>); ignored if kind = Text
    pub video: bool,
    /// deg/s, 0 = no continuous rotation
    pub spin: f64,
    /// fraction of width/s, horizontal drift
    pub drift_x: f64,
    /// fraction of height/s, vertical drift
    pub drift_y: f64,
    pub kind: OverlayKind,
    /// text content (empty for kind = Media)
    pub text: String,
    pub font_family: FontFamily,
    /// vh: resolution-independent, multiplied by `scale`
    pub font_size: f64,
    /// text color, hex
    pub color: String,
    /// part of the auto-cycling rotation (queue overlay)
    pub in_queue: bool,
}

impl Default for Overlay {
    fn default() -> Self {
        Overlay {
            id: String::new(),
            name: String::new(),
            x: 0.5,
            y: 0.5,
            scale: 1.0,
            rotation: 0.0,
            opacity: 1.0,
            blend_mode: "screen".to_string(),
            beat_reactive: false,
            beat_scale: 1.25,
            video: false,
            spin: 0.0,
            drift_x: 0.0,
            drift_y: 0.0,
            kind: OverlayKind::Media,
            text: String::new(),
            font_family: FontFamily::Sans,
            font_size: 8.0,
            color: "#ffffff".to_string(),
            in_queue: false,
        }
    }
}

/// Field-by-field override for `Overlay`, mirroring TS's `Partial<Overlay>`.
/// `id`/`name` are excluded: callers always set those explicitly (see
/// `make_overlay`), whereas TS's positional `partial` argument could in
/// principle also override them via the object spread.
#[derive(Debug, Clone, Default)]
pub struct OverlayPatch {
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub scale: Option<f64>,
    pub rotation: Option<f64>,
    pub opacity: Option<f64>,
    pub blend_mode: Option<String>,
    pub beat_reactive: Option<bool>,
    pub beat_scale: Option<f64>,
    pub video: Option<bool>,
    pub spin: Option<f64>,
    pub drift_x: Option<f64>,
    pub drift_y: Option<f64>,
    pub kind: Option<OverlayKind>,
    pub text: Option<String>,
    pub font_family: Option<FontFamily>,
    pub font_size: Option<f64>,
    pub color: Option<String>,
    pub in_queue: Option<bool>,
}

pub fn apply_overlay_patch(current: Overlay, patch: OverlayPatch) -> Overlay {
    Overlay {
        id: current.id,
        name: current.name,
        x: patch.x.unwrap_or(current.x),
        y: patch.y.unwrap_or(current.y),
        scale: patch.scale.unwrap_or(current.scale),
        rotation: patch.rotation.unwrap_or(current.rotation),
        opacity: patch.opacity.unwrap_or(current.opacity),
        blend_mode: patch.blend_mode.unwrap_or(current.blend_mode),
        beat_reactive: patch.beat_reactive.unwrap_or(current.beat_reactive),
        beat_scale: patch.beat_scale.unwrap_or(current.beat_scale),
        video: patch.video.unwrap_or(current.video),
        spin: patch.spin.unwrap_or(current.spin),
        drift_x: patch.drift_x.unwrap_or(current.drift_x),
        drift_y: patch.drift_y.unwrap_or(current.drift_y),
        kind: patch.kind.unwrap_or(current.kind),
        text: patch.text.unwrap_or(current.text),
        font_family: patch.font_family.unwrap_or(current.font_family),
        font_size: patch.font_size.unwrap_or(current.font_size),
        color: patch.color.unwrap_or(current.color),
        in_queue: patch.in_queue.unwrap_or(current.in_queue),
    }
}

pub fn make_overlay(id: String, name: String, patch: OverlayPatch) -> Overlay {
    let base = Overlay { id, name, ..Overlay::default() };
    apply_overlay_patch(base, patch)
}

/// Amplitude of the drift oscillation on each axis, as a fraction of the
/// composite frame. Port of `OverlayLayer.svelte:58`'s
/// `--drift-x: {(driftX * 60).toFixed(0)}px` (and its `--drift-y` twin),
/// expressed here as a fraction rather than raw pixels so `core` stays
/// resolution-agnostic: the compositor's frame is a fixed 1920x1080
/// (`engine::compositor::COMP_W`/`COMP_H`), so 60 px is exactly 60/1920 of
/// its width and 60/1080 of its height.
pub const DRIFT_AMPLITUDE_X: f64 = 60.0 / 1920.0;
pub const DRIFT_AMPLITUDE_Y: f64 = 60.0 / 1080.0;

/// One overlay's animated placement for one frame: `Overlay`'s static
/// `x`/`y`/`rotation`/`scale` after drift, spin, and the beat pulse have
/// been folded in. Same "pure value, no I/O, no GL" shape as
/// `strobe::strobe_flash_intensity`'s return: `app` computes this each
/// frame and hands it to `engine::compositor`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OverlayTransform {
    /// normalized center X, 0-1 (may drift slightly outside on purpose)
    pub x: f64,
    /// normalized center Y, 0-1
    pub y: f64,
    pub rotation_deg: f64,
    pub scale: f64,
}

/// Port of the three CSS animations `OverlayLayer.svelte` drives per
/// overlay (`spinStyle`, `driftStyle`, and the `beat-pulse` scale bump),
/// evaluated for one point in time instead of handed to the browser's
/// animation engine.
///
/// `elapsed_sec` is monotonic seconds since app start: the same reference
/// every animated overlay shares, so two overlays with identical settings
/// stay in phase with each other (as they do in the browser, where both
/// CSS animations start when the element mounts... approximately; exact
/// per-element start times are not reproducible here and are not worth a
/// per-overlay birth timestamp).
///
/// `beat_pulse` is the shared beat flash (`beatSyncState.beat`, true for
/// 80 ms after each beat); it only matters for a `beat_reactive` overlay.
pub fn overlay_transform_at(overlay: &Overlay, elapsed_sec: f64, beat_pulse: bool) -> OverlayTransform {
    let (drift_x, drift_y) = drift_offset(overlay.drift_x, overlay.drift_y, elapsed_sec);
    OverlayTransform {
        x: overlay.x + drift_x,
        y: overlay.y + drift_y,
        // The web puts `spin` on the anchor and `rotation` on the sprite;
        // with the sprite centered on the anchor those two rotations share
        // a pivot, so they add. (The one visible difference: the web's
        // drift wrapper sits *inside* the spinning anchor, so its drift
        // direction spins too. Here drift stays in screen space: a
        // deliberate simplification.)
        rotation_deg: overlay.rotation + overlay.spin * elapsed_sec,
        scale: if beat_pulse && overlay.beat_reactive { overlay.scale * overlay.beat_scale } else { overlay.scale },
    }
}

/// `animation: od-drift {1/speed}s ease-in-out infinite alternate` from
/// `translate(0,0)` to `translate(driftX*60px, driftY*60px)`, sampled at
/// `elapsed_sec`. `alternate` makes the full cycle twice the declared
/// duration (out, then back); CSS `ease-in-out`
/// (`cubic-bezier(0.42,0,0.58,1)`) is approximated by the smoothstep
/// already used elsewhere in this crate (`snapshot::smoothstep`), same
/// endpoints, same zero derivative at both ends, visually
/// indistinguishable for a slow ambient drift.
fn drift_offset(drift_x: f64, drift_y: f64, elapsed_sec: f64) -> (f64, f64) {
    if drift_x == 0.0 && drift_y == 0.0 {
        return (0.0, 0.0);
    }
    let speed = drift_x.abs().max(drift_y.abs()).max(0.05);
    let leg_sec = 1.0 / speed;
    let phase = (elapsed_sec / leg_sec).rem_euclid(2.0);
    let triangle = if phase < 1.0 { phase } else { 2.0 - phase };
    let eased = triangle * triangle * (3.0 - 2.0 * triangle);
    (drift_x * DRIFT_AMPLITUDE_X * eased, drift_y * DRIFT_AMPLITUDE_Y * eased)
}

/// `Overlay::color` (and every other color in the ported TS) is a CSS hex
/// string, since that's what an `<input type="color">` produces. Parses
/// `#rgb`/`#rrggbb` (with or without the `#`) into 8-bit channels;
/// anything unparseable falls back to white, the same value `Overlay::
/// default()` carries, rather than failing a frame's render.
pub fn parse_hex_color(hex: &str) -> [u8; 3] {
    let s = hex.trim().trim_start_matches('#');
    let nibble = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
    let b = s.as_bytes();
    match b.len() {
        3 => match (nibble(b[0]), nibble(b[1]), nibble(b[2])) {
            (Some(r), Some(g), Some(bl)) => [r * 17, g * 17, bl * 17],
            _ => [255, 255, 255],
        },
        6 => match (
            nibble(b[0]).zip(nibble(b[1])),
            nibble(b[2]).zip(nibble(b[3])),
            nibble(b[4]).zip(nibble(b[5])),
        ) {
            (Some((r1, r0)), Some((g1, g0)), Some((b1, b0))) => [r1 * 16 + r0, g1 * 16 + g0, b1 * 16 + b0],
            _ => [255, 255, 255],
        },
        _ => [255, 255, 255],
    }
}

fn pick_queued_overlays(overlays: &[Overlay]) -> Vec<&Overlay> {
    overlays.iter().filter(|o| o.in_queue).collect()
}

fn retreat_queue_index(current_index: usize, queue_length: usize) -> usize {
    if queue_length == 0 {
        return 0;
    }
    (current_index + queue_length - 1) % queue_length
}

// usize can't go negative, so TS's `index < 0` branch has no equivalent here.
fn clamp_queue_index(index: usize, queue_length: usize) -> usize {
    if queue_length == 0 || index >= queue_length {
        0
    } else {
        index
    }
}

/// Port of OpenDrop-VJ `overlay-queue.ts:41-49` `visibleOverlayIds` (this
/// was never ported, unlike the rest of `overlay-queue.ts`).
/// Overlays to render: every non-queued overlay (always visible) plus the
/// single active overlay from the queue rotation, if at least one overlay is
/// queued. Returns borrowed ids (like `pick_queued_overlays`/
/// `filter_shareable_overlays` in `share_set.rs`) rather than cloning them.
pub fn visible_overlay_ids(overlays: &[Overlay], active_queue_index: usize) -> HashSet<&str> {
    let queued = pick_queued_overlays(overlays);
    let mut ids: HashSet<&str> =
        overlays.iter().filter(|o| !o.in_queue).map(|o| o.id.as_str()).collect();
    if !queued.is_empty() {
        let idx = clamp_queue_index(active_queue_index, queued.len());
        ids.insert(queued[idx].id.as_str());
    }
    ids
}

pub struct OverlayStore {
    pub overlays: Vec<Overlay>,
    pub drag_over: bool,
    pub queue_enabled: bool,
    pub queue_index: usize,
    pub queue_trigger: BeatTriggerConfig,
    pub queue_mode: PlaylistMode,
    rng: Xorshift64,
    /// Rolling-average/cooldown state for `BeatTriggerMode::VolumePeak`
    /// queue advances: the overlay queue's own, deliberately separate
    /// from the two per-deck ones on `Show` (`volume_peak_state_a`/`_b`):
    /// each consumer has its own 500 ms cooldown in the TS source too
    /// (`+page.svelte:558-562`'s module-local `overlayQueueVolumeState`).
    queue_volume_state: VolumePeakState,
}

impl Default for OverlayStore {
    fn default() -> Self {
        Self::new()
    }
}

impl OverlayStore {
    pub fn new() -> Self {
        Self {
            overlays: Vec::new(),
            drag_over: false,
            queue_enabled: false,
            queue_index: 0,
            queue_trigger: default_beat_trigger_config(),
            queue_mode: PlaylistMode::Sequential,
            rng: Xorshift64::default(),
            queue_volume_state: default_volume_peak_state(),
        }
    }

    /// Reseeds the shuffle-mode RNG with real per-launch entropy supplied by
    /// the caller (`core` stays zero-I/O and has no clock of its own). See
    /// `rng.rs`'s module doc comment for why. Called from
    /// `Show::reseed_rng`, which wired this store into `app`.
    pub fn reseed_rng(&mut self, seed: u64) {
        self.rng.reseed(seed);
    }

    pub fn add_text_overlay(&mut self, id: String) -> String {
        let ov = make_overlay(
            id,
            "Text".to_string(),
            OverlayPatch {
                kind: Some(OverlayKind::Text),
                text: Some("Text".to_string()),
                ..Default::default()
            },
        );
        let id = ov.id.clone();
        self.overlays.push(ov);
        id
    }

    /// Positioned overlay dropped on the visualizer. Skips asset
    /// persistence (`saveAsset`): see module docs.
    pub fn add_overlay_at_position(&mut self, id: String, name: String, x: f64, y: f64) -> String {
        let ov = make_overlay(id, name, OverlayPatch { x: Some(x), y: Some(y), ..Default::default() });
        let id = ov.id.clone();
        self.overlays.push(ov);
        id
    }

    /// Skips asset deletion (`deleteAsset`): see module docs.
    pub fn remove_overlay(&mut self, id: &str) {
        self.overlays.retain(|o| o.id != id);
        self.queue_index = clamp_queue_index(self.queue_index, pick_queued_overlays(&self.overlays).len());
    }

    /// `mem::take` moves the found `Overlay` out (leaving `Overlay::default
    /// ()` behind momentarily) so `apply_overlay_patch` can consume its
    /// `String` fields by value instead of cloning them; the taken slot is
    /// immediately overwritten with the patched result on the next line, all
    /// synchronously within this one call, so the transient default is never
    /// observed by any other code. Relies on `Overlay: Default`, see
    /// `Overlay`'s own `impl Default` above.
    pub fn update_overlay(&mut self, id: &str, patch: OverlayPatch) {
        if let Some(ov) = self.overlays.iter_mut().find(|o| o.id == id) {
            let current = std::mem::take(ov);
            *ov = apply_overlay_patch(current, patch);
        }
    }

    pub fn toggle_overlay_queue(&mut self) {
        self.queue_enabled = !self.queue_enabled;
    }

    pub fn set_overlay_queue_mode(&mut self, mode: PlaylistMode) {
        self.queue_mode = mode;
    }

    pub fn update_overlay_queue_trigger(&mut self, patch: BeatTriggerConfigPatch) {
        self.queue_trigger = apply_beat_trigger_patch(self.queue_trigger, patch);
    }

    pub fn advance_overlay_queue(&mut self, direction: i32) {
        let queue_length = pick_queued_overlays(&self.overlays).len();
        self.queue_index = if direction == 1 {
            self.next_queue_index(queue_length)
        } else {
            retreat_queue_index(self.queue_index, queue_length)
        };
    }

    /// The beat half of the auto-cycling queue: port of
    /// `beat-tempo-actions.ts:71-76`, the last of that function's three
    /// `shouldTriggerOnBeat` blocks (the other two, per-deck playlist
    /// advance, are already `Show::maybe_advance_on_beat`). Called on every
    /// beat; `should_trigger_on_beat` itself returns `false` whenever the
    /// trigger is in `VolumePeak` mode, so the two halves can never both
    /// fire for the same configuration.
    pub fn maybe_advance_on_beat(&mut self, beat_count: i64) {
        if !self.queue_enabled || !should_trigger_on_beat(beat_count, self.queue_trigger) {
            return;
        }
        self.advance_overlay_queue(1);
    }

    /// The volume-peak half of the same queue: port of
    /// `+page.svelte:558-562`, the overlay-queue twin of
    /// `Show::check_one_volume_peak`. `rms` is the current tick's VU level,
    /// `now_ms` a monotonic millisecond timestamp (the 500 ms cooldown in
    /// `detect_volume_peak` is measured against it).
    pub fn maybe_advance_on_volume_peak(&mut self, rms: f64, now_ms: f64) {
        if !self.queue_enabled || self.queue_trigger.mode != BeatTriggerMode::VolumePeak {
            return;
        }
        let result = detect_volume_peak(rms, self.queue_volume_state, self.queue_trigger.sensitivity, now_ms);
        self.queue_volume_state = result.next;
        if result.triggered {
            self.advance_overlay_queue(1);
        }
    }

    /// DOM dragover handling, minus the DOM: `has_files` is the caller's
    /// `dataTransfer.types.includes('Files')` check; the return value tells
    /// the caller whether it should call `preventDefault()`.
    pub fn on_visualizer_drag_over(&mut self, has_files: bool) -> bool {
        if !has_files {
            return false;
        }
        self.drag_over = true;
        true
    }

    fn next_queue_index(&mut self, queue_length: usize) -> usize {
        if queue_length == 0 {
            return 0;
        }
        match self.queue_mode {
            PlaylistMode::Sequential => (self.queue_index + 1) % queue_length,
            PlaylistMode::Shuffle => self.random_queue_index(queue_length),
        }
    }

    fn random_queue_index(&mut self, queue_length: usize) -> usize {
        if queue_length <= 1 {
            return 0;
        }
        loop {
            let idx = (self.rng.next_f64() * queue_length as f64) as usize;
            let idx = idx.min(queue_length - 1);
            if idx != self.queue_index {
                return idx;
            }
        }
    }
}

#[cfg(test)]
mod tests;
