//! Port of OpenDrop-VJ `src/lib/engine/playlist.ts` and
//! `src/lib/engine/playlist-store.svelte.ts`, merged into one file: the
//! `PlaylistEngine` state machine plus the in-memory playlist management
//! (item CRUD, mode/interval, per-deck start/stop/next/prev) that wrapped it
//! in a Svelte store.
//!
//! Three adaptations for a zero-I/O, fully unit-testable `core`:
//! - The TS engine drives auto-advance with `setTimeout`. Real timers are
//!   I/O, so `PlaylistEngine` exposes `tick(delta_ms)` instead: the caller
//!   (a later, I/O-capable layer) reports elapsed time, same shape as
//!   `Clock::step` in `clock.rs`.
//! - Shuffle mode used `Math.random()`. A zero-I/O crate has no entropy
//!   source, so it uses a small deterministic xorshift64 PRNG instead. No
//!   ported test asserts on actual randomness: only "never repeats the
//!   current index": so determinism costs nothing here.
//! - `exportPlaylists`/`importPlaylists` are Blob/File/DOM download-and-read
//!   operations: file I/O, not in-memory list management: so they're
//!   dropped rather than ported.
//!
//! Svelte `$state` reactivity is likewise dropped: `PlaylistStore` fields
//! are plain struct fields mutated through `&mut self` methods.

use crate::commands::Deck;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaylistMode {
    Sequential,
    Shuffle,
}

pub struct PlaylistEngine {
    items: Vec<String>,
    mode: PlaylistMode,
    interval_ms: f64,
    index: usize,
    playing: bool,
    elapsed_ms: f64,
    rng_state: u64,
    on_preset: Box<dyn FnMut(&str)>,
}

impl PlaylistEngine {
    pub fn new(
        items: Vec<String>,
        mode: PlaylistMode,
        interval_ms: f64,
        on_preset: Box<dyn FnMut(&str)>,
    ) -> Self {
        Self {
            items,
            mode,
            interval_ms,
            index: 0,
            playing: false,
            elapsed_ms: 0.0,
            rng_state: 0x9E3779B97F4A7C15,
            on_preset,
        }
    }

    pub fn playing(&self) -> bool {
        self.playing
    }

    pub fn current_index(&self) -> usize {
        self.index
    }

    pub fn interval_ms(&self) -> f64 {
        self.interval_ms
    }

    pub fn start(&mut self) {
        if self.playing || self.items.is_empty() {
            return;
        }
        self.playing = true;
        self.elapsed_ms = 0.0;
        (self.on_preset)(&self.items[self.index]);
    }

    pub fn stop(&mut self) {
        self.playing = false;
        self.elapsed_ms = 0.0;
    }

    pub fn next(&mut self) {
        if self.items.is_empty() {
            return;
        }
        self.advance_index();
        (self.on_preset)(&self.items[self.index]);
        // A manual call cancels whatever was pending and restarts a full
        // interval from now: same effect as the TS stop()+schedule() dance.
        if self.playing {
            self.elapsed_ms = 0.0;
        }
    }

    pub fn prev(&mut self) {
        if self.items.is_empty() {
            return;
        }
        self.index = (self.index + self.items.len() - 1) % self.items.len();
        (self.on_preset)(&self.items[self.index]);
    }

    pub fn set_items(&mut self, items: Vec<String>) {
        self.items = items;
        if self.index >= self.items.len() {
            self.index = 0;
        }
    }

    pub fn set_interval(&mut self, ms: f64) {
        self.interval_ms = ms;
    }

    pub fn set_mode(&mut self, mode: PlaylistMode) {
        self.mode = mode;
    }

    pub fn destroy(&mut self) {
        self.stop();
    }

    /// Caller-driven replacement for `setTimeout`: advances the internal
    /// clock by `delta_ms`, firing as many auto-advances as fit.
    pub fn tick(&mut self, delta_ms: f64) {
        if !self.playing || !self.interval_ms.is_finite() {
            return;
        }
        self.elapsed_ms += delta_ms;
        while self.elapsed_ms >= self.interval_ms {
            self.elapsed_ms -= self.interval_ms;
            self.advance_index();
            (self.on_preset)(&self.items[self.index]);
        }
    }

    fn advance_index(&mut self) {
        self.index = match self.mode {
            PlaylistMode::Sequential => (self.index + 1) % self.items.len(),
            PlaylistMode::Shuffle => self.random_index(),
        };
    }

    fn random_index(&mut self) -> usize {
        if self.items.len() <= 1 {
            return 0;
        }
        loop {
            let idx = (self.next_random() * self.items.len() as f64) as usize;
            let idx = idx.min(self.items.len() - 1);
            if idx != self.index {
                return idx;
            }
        }
    }

    fn next_random(&mut self) -> f64 {
        self.rng_state ^= self.rng_state << 13;
        self.rng_state ^= self.rng_state >> 7;
        self.rng_state ^= self.rng_state << 17;
        (self.rng_state >> 11) as f64 / (1u64 << 53) as f64
    }
}

pub struct PlaylistStore {
    pub interval_sec: f64,
    pub mode: PlaylistMode,
    pub a_playing: bool,
    pub b_playing: bool,
    pub a_items: Vec<String>,
    pub b_items: Vec<String>,
    engine_a: Option<PlaylistEngine>,
    engine_b: Option<PlaylistEngine>,
}

impl Default for PlaylistStore {
    fn default() -> Self {
        Self::new()
    }
}

impl PlaylistStore {
    pub fn new() -> Self {
        Self {
            interval_sec: 10.0,
            mode: PlaylistMode::Sequential,
            a_playing: false,
            b_playing: false,
            a_items: Vec::new(),
            b_items: Vec::new(),
            engine_a: None,
            engine_b: None,
        }
    }

    /// Register the two engine instances once they've been constructed
    /// (the caller's equivalent of startVisualizer).
    pub fn set_engines(&mut self, a: PlaylistEngine, b: PlaylistEngine) {
        self.engine_a = Some(a);
        self.engine_b = Some(b);
    }

    pub fn destroy_engines(&mut self) {
        if let Some(engine) = self.engine_a.as_mut() {
            engine.destroy();
        }
        if let Some(engine) = self.engine_b.as_mut() {
            engine.destroy();
        }
        self.engine_a = None;
        self.engine_b = None;
    }

    pub fn engine_a_mut(&mut self) -> Option<&mut PlaylistEngine> {
        self.engine_a.as_mut()
    }

    pub fn engine_b_mut(&mut self) -> Option<&mut PlaylistEngine> {
        self.engine_b.as_mut()
    }

    pub fn add_to_playlist(&mut self, deck: Deck, name: String) {
        let (items, engine) = self.deck_parts_mut(deck);
        if items.contains(&name) {
            return;
        }
        items.push(name);
        if let Some(engine) = engine {
            engine.set_items(items.clone());
        }
    }

    pub fn remove_from_playlist(&mut self, deck: Deck, name: &str) {
        let (items, engine) = self.deck_parts_mut(deck);
        items.retain(|n| n.as_str() != name);
        if let Some(engine) = engine {
            engine.set_items(items.clone());
        }
    }

    pub fn toggle_playlist(&mut self, deck: Deck) {
        let interval_ms = self.interval_sec * 1000.0;
        let mode = self.mode;
        let (engine, playing_flag) = match deck {
            Deck::A => (&mut self.engine_a, &mut self.a_playing),
            Deck::B => (&mut self.engine_b, &mut self.b_playing),
        };
        let engine = match engine {
            Some(engine) => engine,
            None => return,
        };
        engine.set_interval(interval_ms);
        engine.set_mode(mode);
        if engine.playing() {
            engine.stop();
        } else {
            engine.start();
        }
        *playing_flag = engine.playing();
    }

    pub fn playlist_next(&mut self, deck: Deck) {
        if let Some(engine) = self.engine_mut(deck) {
            engine.next();
        }
    }

    pub fn playlist_prev(&mut self, deck: Deck) {
        if let Some(engine) = self.engine_mut(deck) {
            engine.prev();
        }
    }

    /// Used by beat-sync toggles (Infinity = fully beat-driven, no own timer).
    pub fn set_beat_sync_interval(&mut self, deck: Deck, ms: f64) {
        if let Some(engine) = self.engine_mut(deck) {
            engine.set_interval(ms);
        }
    }

    fn engine_mut(&mut self, deck: Deck) -> Option<&mut PlaylistEngine> {
        match deck {
            Deck::A => self.engine_a.as_mut(),
            Deck::B => self.engine_b.as_mut(),
        }
    }

    fn deck_parts_mut(&mut self, deck: Deck) -> (&mut Vec<String>, &mut Option<PlaylistEngine>) {
        match deck {
            Deck::A => (&mut self.a_items, &mut self.engine_a),
            Deck::B => (&mut self.b_items, &mut self.engine_b),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn items(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[allow(clippy::type_complexity)]
    fn spy() -> (Rc<RefCell<Vec<String>>>, Box<dyn FnMut(&str)>) {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let calls_clone = calls.clone();
        let cb: Box<dyn FnMut(&str)> =
            Box::new(move |name: &str| calls_clone.borrow_mut().push(name.to_string()));
        (calls, cb)
    }

    mod engine {
        use super::*;

        #[test]
        fn start_loads_the_current_preset_immediately() {
            let (calls, cb) = spy();
            let mut pl =
                PlaylistEngine::new(items(&["A", "B", "C"]), PlaylistMode::Sequential, 5000.0, cb);
            pl.start();
            assert_eq!(*calls.borrow(), vec!["A"]);
        }

        #[test]
        fn start_schedules_the_next_one_after_interval_ms() {
            let (calls, cb) = spy();
            let mut pl =
                PlaylistEngine::new(items(&["A", "B", "C"]), PlaylistMode::Sequential, 5000.0, cb);
            pl.start();
            pl.tick(5000.0);
            assert_eq!(*calls.borrow(), vec!["A", "B"]);
        }

        #[test]
        fn sequential_cycle_loops_around() {
            let (calls, cb) = spy();
            let mut pl = PlaylistEngine::new(items(&["A", "B"]), PlaylistMode::Sequential, 1000.0, cb);
            pl.start();
            pl.tick(1000.0);
            pl.tick(1000.0);
            pl.tick(1000.0);
            assert_eq!(*calls.borrow(), vec!["A", "B", "A", "B"]);
        }

        #[test]
        fn stop_stops_the_cycle() {
            let (calls, cb) = spy();
            let mut pl =
                PlaylistEngine::new(items(&["A", "B", "C"]), PlaylistMode::Sequential, 1000.0, cb);
            pl.start();
            pl.stop();
            pl.tick(5000.0);
            assert_eq!(*calls.borrow(), vec!["A"]);
        }

        #[test]
        fn next_advances_manually() {
            let (calls, cb) = spy();
            let mut pl =
                PlaylistEngine::new(items(&["A", "B", "C"]), PlaylistMode::Sequential, 5000.0, cb);
            pl.next();
            assert_eq!(*calls.borrow(), vec!["B"]);
        }

        #[test]
        fn prev_goes_back_manually() {
            let (calls, cb) = spy();
            let mut pl =
                PlaylistEngine::new(items(&["A", "B", "C"]), PlaylistMode::Sequential, 5000.0, cb);
            pl.next();
            pl.prev();
            assert_eq!(*calls.borrow(), vec!["B", "A"]);
        }

        #[test]
        fn prev_from_index_0_goes_to_the_last_one() {
            let (calls, cb) = spy();
            let mut pl =
                PlaylistEngine::new(items(&["A", "B", "C"]), PlaylistMode::Sequential, 5000.0, cb);
            pl.prev();
            assert_eq!(*calls.borrow(), vec!["C"]);
        }

        #[test]
        fn does_not_start_if_the_list_is_empty() {
            let (calls, cb) = spy();
            let mut pl = PlaylistEngine::new(Vec::new(), PlaylistMode::Sequential, 1000.0, cb);
            pl.start();
            assert!(!pl.playing());
            assert!(calls.borrow().is_empty());
        }

        #[test]
        fn set_items_resets_index_if_out_of_bounds() {
            let (calls, cb) = spy();
            let mut pl =
                PlaylistEngine::new(items(&["A", "B", "C"]), PlaylistMode::Sequential, 1000.0, cb);
            pl.next();
            pl.next();
            pl.set_items(items(&["X"]));
            pl.start();
            assert_eq!(calls.borrow().last(), Some(&"X".to_string()));
        }

        #[test]
        fn set_interval_updates_the_cycle_duration() {
            let (calls, cb) = spy();
            let mut pl = PlaylistEngine::new(items(&["A", "B"]), PlaylistMode::Sequential, 5000.0, cb);
            pl.set_interval(1000.0);
            pl.start();
            pl.tick(1000.0);
            assert_eq!(calls.borrow().last(), Some(&"B".to_string()));
        }

        #[test]
        fn set_interval_infinity_disables_automatic_advance() {
            let (calls, cb) = spy();
            let mut pl = PlaylistEngine::new(
                items(&["A", "B", "C"]),
                PlaylistMode::Sequential,
                f64::INFINITY,
                cb,
            );
            pl.start();
            pl.tick(100_000.0);
            assert_eq!(*calls.borrow(), vec!["A"]);
        }

        #[test]
        fn playing_correctly_reflects_start_stop() {
            let (_, cb) = spy();
            let mut pl = PlaylistEngine::new(items(&["A", "B"]), PlaylistMode::Sequential, 1000.0, cb);
            assert!(!pl.playing());
            pl.start();
            assert!(pl.playing());
            pl.stop();
            assert!(!pl.playing());
        }

        #[test]
        fn shuffle_mode_never_repeats_the_current_index() {
            let (_, cb) = spy();
            let mut pl = PlaylistEngine::new(items(&["A", "B", "C"]), PlaylistMode::Shuffle, 1000.0, cb);
            let mut prev = pl.current_index();
            for _ in 0..30 {
                pl.next();
                assert_ne!(pl.current_index(), prev);
                prev = pl.current_index();
            }
        }

        #[test]
        fn shuffle_mode_with_a_single_item_always_picks_index_0() {
            let (calls, cb) = spy();
            let mut pl = PlaylistEngine::new(items(&["Only"]), PlaylistMode::Shuffle, 1000.0, cb);
            for _ in 0..5 {
                pl.next();
            }
            assert_eq!(pl.current_index(), 0);
            assert_eq!(calls.borrow().len(), 5);
        }
    }

    mod store {
        use super::*;

        mod add_remove {
            use super::*;

            #[test]
            fn adds_a_preset_to_playlist_a_without_duplicating_it() {
                let mut store = PlaylistStore::new();
                store.add_to_playlist(Deck::A, "preset1".to_string());
                store.add_to_playlist(Deck::A, "preset1".to_string());
                assert_eq!(store.a_items, vec!["preset1"]);
            }

            #[test]
            fn adds_a_preset_to_playlist_b_independently_of_a() {
                let mut store = PlaylistStore::new();
                store.add_to_playlist(Deck::B, "presetX".to_string());
                assert_eq!(store.b_items, vec!["presetX"]);
                assert!(store.a_items.is_empty());
            }

            #[test]
            fn removes_a_preset_from_the_targeted_playlist() {
                let mut store = PlaylistStore::new();
                store.add_to_playlist(Deck::A, "p1".to_string());
                store.add_to_playlist(Deck::A, "p2".to_string());
                store.remove_from_playlist(Deck::A, "p1");
                assert_eq!(store.a_items, vec!["p2"]);
            }

            #[test]
            fn propagates_set_items_to_the_active_engine() {
                let mut store = PlaylistStore::new();
                let (calls_a, cb_a) = spy();
                let (_, cb_b) = spy();
                let engine_a = PlaylistEngine::new(Vec::new(), PlaylistMode::Sequential, 1000.0, cb_a);
                let engine_b = PlaylistEngine::new(Vec::new(), PlaylistMode::Sequential, 1000.0, cb_b);
                store.set_engines(engine_a, engine_b);
                store.add_to_playlist(Deck::A, "p1".to_string());
                store.engine_a_mut().unwrap().start();
                assert_eq!(*calls_a.borrow(), vec!["p1"]);
            }
        }

        mod toggle_playlist {
            use super::*;

            #[test]
            fn starts_then_stops_and_reflects_playing_in_the_store() {
                let mut store = PlaylistStore::new();
                let (_, cb_a) = spy();
                let (_, cb_b) = spy();
                let engine_a = PlaylistEngine::new(items(&["p1"]), PlaylistMode::Sequential, 1000.0, cb_a);
                let engine_b = PlaylistEngine::new(Vec::new(), PlaylistMode::Sequential, 1000.0, cb_b);
                store.set_engines(engine_a, engine_b);
                store.toggle_playlist(Deck::A);
                assert!(store.a_playing);
                store.toggle_playlist(Deck::A);
                assert!(!store.a_playing);
            }

            #[test]
            fn does_nothing_if_the_engine_has_not_been_created_yet() {
                let mut store = PlaylistStore::new();
                store.toggle_playlist(Deck::A);
                assert!(!store.a_playing);
            }

            #[test]
            fn applies_the_current_interval_and_mode_before_starting() {
                let mut store = PlaylistStore::new();
                let (_, cb_a) = spy();
                let (calls_b, cb_b) = spy();
                let engine_a = PlaylistEngine::new(Vec::new(), PlaylistMode::Sequential, 1000.0, cb_a);
                let engine_b =
                    PlaylistEngine::new(items(&["x", "y"]), PlaylistMode::Sequential, 5000.0, cb_b);
                store.set_engines(engine_a, engine_b);
                store.mode = PlaylistMode::Sequential;
                store.interval_sec = 2.0;
                store.toggle_playlist(Deck::B);
                store.engine_b_mut().unwrap().tick(2000.0);
                assert_eq!(*calls_b.borrow(), vec!["x", "y"]);
            }
        }

        mod next_prev {
            use super::*;

            #[test]
            fn advances_and_goes_back_on_the_correct_deck() {
                let mut store = PlaylistStore::new();
                let (calls_a, cb_a) = spy();
                let (_, cb_b) = spy();
                let engine_a =
                    PlaylistEngine::new(items(&["p1", "p2"]), PlaylistMode::Sequential, 1000.0, cb_a);
                let engine_b = PlaylistEngine::new(Vec::new(), PlaylistMode::Sequential, 1000.0, cb_b);
                store.set_engines(engine_a, engine_b);
                store.playlist_next(Deck::A);
                store.playlist_prev(Deck::A);
                assert_eq!(*calls_a.borrow(), vec!["p2", "p1"]);
            }
        }

        mod beat_sync_interval {
            use super::*;

            #[test]
            fn only_updates_the_targeted_decks_engine() {
                let mut store = PlaylistStore::new();
                let (_, cb_a) = spy();
                let (_, cb_b) = spy();
                let engine_a =
                    PlaylistEngine::new(items(&["p1", "p2"]), PlaylistMode::Sequential, 1000.0, cb_a);
                let engine_b = PlaylistEngine::new(Vec::new(), PlaylistMode::Sequential, 1000.0, cb_b);
                store.set_engines(engine_a, engine_b);
                store.set_beat_sync_interval(Deck::A, f64::INFINITY);
                assert_eq!(store.engine_a_mut().unwrap().interval_ms(), f64::INFINITY);
                assert_eq!(store.engine_b_mut().unwrap().interval_ms(), 1000.0);
            }
        }

        #[test]
        fn destroy_engines_clears_both_slots() {
            let mut store = PlaylistStore::new();
            let (_, cb_a) = spy();
            let (_, cb_b) = spy();
            let engine_a = PlaylistEngine::new(Vec::new(), PlaylistMode::Sequential, 1000.0, cb_a);
            let engine_b = PlaylistEngine::new(Vec::new(), PlaylistMode::Sequential, 1000.0, cb_b);
            store.set_engines(engine_a, engine_b);
            store.destroy_engines();
            assert!(store.engine_a_mut().is_none());
            assert!(store.engine_b_mut().is_none());
        }
    }
}
