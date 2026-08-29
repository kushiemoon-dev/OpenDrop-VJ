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
//!   storage/file I/O, not in-memory list management: dropped, they
//!   belong to a later I/O-capable phase. `add_overlay_at_position` and
//!   `remove_overlay` keep only the pure list-bookkeeping half of their TS
//!   counterparts.
//! - Queue shuffle used `Math.random()`. Same rationale as `playlist.rs`:
//!   a small deterministic xorshift64 PRNG instead, since no ported test
//!   asserts on actual randomness.
//!
//! Svelte `$state` reactivity is likewise dropped: `OverlayStore` fields
//! are plain struct fields mutated through `&mut self` methods.

use std::collections::HashSet;

use crate::beat_trigger::{
    apply_beat_trigger_patch, default_beat_trigger_config, BeatTriggerConfig,
    BeatTriggerConfigPatch,
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
    /// true = video asset (rendered as <video> instead of <img>): ignored if kind = Text
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

/// Port of OpenDrop-VJ `overlay-queue.ts:41-49` `visibleOverlayIds`: Finding
/// I3 (this was never ported, unlike the rest of `overlay-queue.ts`).
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
        }
    }

    /// Reseeds the shuffle-mode RNG with real per-launch entropy supplied by
    /// the caller (`core` stays zero-I/O and has no clock of its own). See
    /// `rng.rs`'s module doc comment: whole-branch review Finding I4. Not
    /// currently called by `app`: `OverlayStore` isn't wired into `app` yet
    /// (see `share_set.rs`'s module doc comment), but this keeps it correct
    /// for whenever that wiring lands.
    pub fn reseed_rng(&mut self, seed: u64) {
        self.rng.reseed(seed);
    }

    pub fn add_text_overlay(&mut self, id: String) -> String {
        let ov = make_overlay(
            id,
            "Texte".to_string(),
            OverlayPatch {
                kind: Some(OverlayKind::Text),
                text: Some("Texte".to_string()),
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
    /// observed by any other code. Relies on `Overlay: Default`: see
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
mod tests {
    use super::*;

    mod make_overlay_tests {
        use super::*;

        #[test]
        fn defaults_to_kind_media_backward_compatible() {
            let ov = make_overlay("id-1".to_string(), "mon-image".to_string(), OverlayPatch::default());
            assert_eq!(ov.kind, OverlayKind::Media);
            assert!(!ov.video);
            assert_eq!(ov.text, "");
        }

        #[test]
        fn creates_a_text_overlay_with_the_correct_defaults() {
            let ov = make_overlay(
                "id-1".to_string(),
                "Texte".to_string(),
                OverlayPatch {
                    kind: Some(OverlayKind::Text),
                    text: Some("Hello".to_string()),
                    ..Default::default()
                },
            );
            assert_eq!(ov.kind, OverlayKind::Text);
            assert_eq!(ov.text, "Hello");
            assert_eq!(ov.font_family, FontFamily::Sans);
            assert_eq!(ov.font_size, 8.0);
            assert_eq!(ov.color, "#ffffff");
        }

        #[test]
        fn shared_fields_remain_unchanged() {
            let ov = make_overlay(
                "id-1".to_string(),
                "Texte".to_string(),
                OverlayPatch { kind: Some(OverlayKind::Text), ..Default::default() },
            );
            assert_eq!(ov.x, 0.5);
            assert_eq!(ov.y, 0.5);
            assert_eq!(ov.scale, 1.0);
            assert_eq!(ov.opacity, 1.0);
            assert_eq!(ov.spin, 0.0);
            assert_eq!(ov.drift_x, 0.0);
            assert_eq!(ov.drift_y, 0.0);
        }

        #[test]
        fn partial_override_replaces_only_the_provided_fields() {
            let ov = make_overlay(
                "id-1".to_string(),
                "Texte".to_string(),
                OverlayPatch {
                    kind: Some(OverlayKind::Text),
                    font_family: Some(FontFamily::Impact),
                    color: Some("#ff2d78".to_string()),
                    ..Default::default()
                },
            );
            assert_eq!(ov.font_family, FontFamily::Impact);
            assert_eq!(ov.color, "#ff2d78");
            assert_eq!(ov.font_size, 8.0);
        }

        #[test]
        fn in_queue_defaults_to_false() {
            let ov = make_overlay("id-1".to_string(), "mon-image".to_string(), OverlayPatch::default());
            assert!(!ov.in_queue);
        }
    }

    mod overlay_store_tests {
        use super::*;

        #[test]
        fn add_text_overlay_adds_a_text_overlay_and_returns_its_id() {
            let mut store = OverlayStore::new();
            let id = store.add_text_overlay("id-1".to_string());
            assert_eq!(store.overlays.len(), 1);
            assert_eq!(store.overlays[0].kind, OverlayKind::Text);
            assert_eq!(store.overlays[0].id, id);
        }

        #[test]
        fn add_overlay_at_position_adds_a_positioned_overlay() {
            let mut store = OverlayStore::new();
            store.add_overlay_at_position("id-1".to_string(), "photo".to_string(), 0.2, 0.8);
            assert_eq!(store.overlays.len(), 1);
            assert_eq!(store.overlays[0].x, 0.2);
            assert_eq!(store.overlays[0].y, 0.8);
        }

        #[test]
        fn remove_overlay_deletes_the_overlay_from_the_list() {
            let mut store = OverlayStore::new();
            let id = store.add_text_overlay("id-1".to_string());
            store.remove_overlay(&id);
            assert!(store.overlays.is_empty());
        }

        #[test]
        fn remove_overlay_re_clamps_queue_index_if_the_queue_shrinks() {
            let mut store = OverlayStore::new();
            store.overlays = vec![
                Overlay { id: "a".to_string(), in_queue: true, ..Default::default() },
                Overlay { id: "b".to_string(), in_queue: true, ..Default::default() },
            ];
            store.queue_index = 1;
            store.remove_overlay("b");
            assert_eq!(store.queue_index, 0);
        }

        #[test]
        fn update_overlay_merges_a_patch_onto_the_correct_overlay_only() {
            let mut store = OverlayStore::new();
            let id = store.add_text_overlay("id-1".to_string());
            store.update_overlay(&id, OverlayPatch { opacity: Some(0.4), ..Default::default() });
            assert_eq!(store.overlays[0].opacity, 0.4);
            assert_eq!(store.overlays[0].text, "Texte");
        }

        #[test]
        fn toggle_overlay_queue_toggles_enabled() {
            let mut store = OverlayStore::new();
            assert!(!store.queue_enabled);
            store.toggle_overlay_queue();
            assert!(store.queue_enabled);
            store.toggle_overlay_queue();
            assert!(!store.queue_enabled);
        }

        #[test]
        fn set_overlay_queue_mode_replaces_the_mode() {
            let mut store = OverlayStore::new();
            store.set_overlay_queue_mode(PlaylistMode::Shuffle);
            assert_eq!(store.queue_mode, PlaylistMode::Shuffle);
        }

        #[test]
        fn update_overlay_queue_trigger_merges_and_re_clamps_via_apply_beat_trigger_patch() {
            let mut store = OverlayStore::new();
            store.update_overlay_queue_trigger(BeatTriggerConfigPatch {
                beats_per_change: Some(100),
                ..Default::default()
            });
            assert_eq!(store.queue_trigger.beats_per_change, 64);
        }

        #[test]
        fn advance_overlay_queue_advances_sequentially_among_the_queued_overlays() {
            let mut store = OverlayStore::new();
            store.overlays = vec![
                Overlay { id: "a".to_string(), in_queue: true, ..Default::default() },
                Overlay { id: "b".to_string(), in_queue: true, ..Default::default() },
                Overlay { id: "c".to_string(), in_queue: false, ..Default::default() },
            ];
            store.queue_mode = PlaylistMode::Sequential;
            store.queue_index = 0;
            store.advance_overlay_queue(1);
            assert_eq!(store.queue_index, 1);
        }

        #[test]
        fn advance_overlay_queue_goes_backward_regardless_of_mode() {
            let mut store = OverlayStore::new();
            store.overlays = vec![
                Overlay { id: "a".to_string(), in_queue: true, ..Default::default() },
                Overlay { id: "b".to_string(), in_queue: true, ..Default::default() },
            ];
            store.queue_index = 1;
            store.advance_overlay_queue(-1);
            assert_eq!(store.queue_index, 0);
        }

        #[test]
        fn advance_overlay_queue_shuffle_never_repeats_the_current_index() {
            let mut store = OverlayStore::new();
            store.overlays = vec![
                Overlay { id: "a".to_string(), in_queue: true, ..Default::default() },
                Overlay { id: "b".to_string(), in_queue: true, ..Default::default() },
                Overlay { id: "c".to_string(), in_queue: true, ..Default::default() },
            ];
            store.queue_mode = PlaylistMode::Shuffle;
            let mut prev = store.queue_index;
            for _ in 0..30 {
                store.advance_overlay_queue(1);
                assert_ne!(store.queue_index, prev);
                prev = store.queue_index;
            }
        }

        #[test]
        fn advance_overlay_queue_shuffle_with_a_single_queued_overlay_always_picks_index_0() {
            let mut store = OverlayStore::new();
            store.overlays = vec![Overlay { id: "only".to_string(), in_queue: true, ..Default::default() }];
            store.queue_mode = PlaylistMode::Shuffle;
            for _ in 0..5 {
                store.advance_overlay_queue(1);
            }
            assert_eq!(store.queue_index, 0);
        }

        #[test]
        fn on_visualizer_drag_over_activates_drag_over_only_when_files_are_dragged() {
            let mut store = OverlayStore::new();
            let should_prevent_default = store.on_visualizer_drag_over(true);
            assert!(store.drag_over);
            assert!(should_prevent_default);
        }

        #[test]
        fn on_visualizer_drag_over_ignores_a_dragover_without_files() {
            let mut store = OverlayStore::new();
            let should_prevent_default = store.on_visualizer_drag_over(false);
            assert!(!store.drag_over);
            assert!(!should_prevent_default);
        }

        #[test]
        fn reseed_rng_makes_shuffle_draws_actually_differ_across_seeds() {
            let mut store = OverlayStore::new();
            store.overlays = vec![
                Overlay { id: "a".to_string(), in_queue: true, ..Default::default() },
                Overlay { id: "b".to_string(), in_queue: true, ..Default::default() },
                Overlay { id: "c".to_string(), in_queue: true, ..Default::default() },
                Overlay { id: "d".to_string(), in_queue: true, ..Default::default() },
                Overlay { id: "e".to_string(), in_queue: true, ..Default::default() },
            ];
            store.queue_mode = PlaylistMode::Shuffle;
            let draws = |store: &mut OverlayStore, seed: u64| -> Vec<usize> {
                store.reseed_rng(seed);
                store.queue_index = 0;
                (0..10)
                    .map(|_| {
                        store.advance_overlay_queue(1);
                        store.queue_index
                    })
                    .collect()
            };
            let seq_1 = draws(&mut store, 1);
            let seq_2 = draws(&mut store, 2);
            assert_ne!(seq_1, seq_2);
        }
    }

    /// Port of `overlay-queue.test.ts:81-108`'s `visibleOverlayIds` tests.
    mod visible_overlay_ids_tests {
        use super::*;

        fn overlay(id: &str, in_queue: bool) -> Overlay {
            Overlay { id: id.to_string(), in_queue, ..Default::default() }
        }

        #[test]
        fn zero_checked_all_unchecked_ones_are_visible() {
            let overlays = vec![overlay("a", false), overlay("b", false)];
            let ids = visible_overlay_ids(&overlays, 0);
            assert_eq!(ids, HashSet::from(["a", "b"]));
        }

        #[test]
        fn one_checked_always_visible_plus_the_unchecked_ones() {
            let overlays = vec![overlay("a", true), overlay("b", false)];
            let ids = visible_overlay_ids(&overlays, 0);
            assert_eq!(ids, HashSet::from(["a", "b"]));
        }

        #[test]
        fn multiple_checked_only_the_one_at_the_active_index_plus_the_unchecked_ones() {
            let overlays = vec![overlay("a", true), overlay("b", true), overlay("c", false)];
            let ids = visible_overlay_ids(&overlays, 1);
            assert_eq!(ids, HashSet::from(["b", "c"]));
        }

        #[test]
        fn out_of_bounds_index_clean_fallback_no_crash_first_checked_one_visible() {
            let overlays = vec![overlay("a", true), overlay("b", true)];
            let ids = visible_overlay_ids(&overlays, 99);
            assert_eq!(ids, HashSet::from(["a"]));
        }
    }
}
