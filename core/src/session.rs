//! Everything the app restores after a restart, as plain data: the shareable
//! set (mix, colour, snapshots, timeline, overlays, beat triggers) plus the
//! parts a share link deliberately leaves out, such as which preset each deck
//! was running, the playlists, the strobe and the LFO routing.
//!
//! `core` has no I/O and no serde; `opendrop_io::session_codec` turns a
//! [`Session`] into JSON and back.

use crate::commands::Deck;
use crate::lfo::{LfoSlot, LFO_SLOTS};
use crate::playlist::PlaylistMode;
use crate::share_set::SharedSet;
use crate::show::Show;
use crate::strobe::StrobeState;

pub struct PlaylistState {
    pub interval_sec: f64,
    pub mode: PlaylistMode,
    pub a_items: Vec<String>,
    pub b_items: Vec<String>,
}

pub struct Session {
    /// Overlays are kept whole here, including the ones backed by a local
    /// file that a share link has to drop.
    pub shared: SharedSet,
    /// Preset loaded on each deck, empty when none.
    pub deck_presets: [String; 4],
    pub selected_slot: usize,
    pub lock_a: bool,
    pub lock_b: bool,
    pub playlists: PlaylistState,
    pub strobe: StrobeState,
    pub lfo_slots: [LfoSlot; LFO_SLOTS],
}

impl Show {
    pub fn capture_session(&self, deck_presets: [String; 4], transition_time: f64) -> Session {
        let shared = SharedSet {
            name: String::new(),
            preset_a: String::new(),
            preset_b: String::new(),
            deck_bus: self.deck_bus,
            crossfader: self.crossfader,
            transition_time,
            color_params_a: self.color_params_a,
            color_params_b: self.color_params_b,
            slot_composites: self.slot_composites,
            time_params: self.time_params,
            q_var_params: self.q_var_params,
            snapshots: self.snapshot_slots.clone(),
            snapshot_recall_duration: self.snapshot_recall_duration_sec,
            timeline_keyframes: self.timeline_keyframes.clone(),
            overlays: self.overlay_store.overlays.clone(),
            beat_trigger_a: self.beat_trigger_a,
            beat_trigger_b: self.beat_trigger_b,
            beat_sync_a: self.beat_sync_a,
            beat_sync_b: self.beat_sync_b,
            overlay_queue_enabled: self.overlay_store.queue_enabled,
            overlay_queue_trigger: self.overlay_store.queue_trigger,
        };
        Session {
            shared,
            deck_presets,
            selected_slot: self.selected_slot,
            lock_a: self.lock_a,
            lock_b: self.lock_b,
            playlists: PlaylistState {
                interval_sec: self.playlists.interval_sec,
                mode: self.playlists.mode,
                a_items: self.playlists.a_items.clone(),
                b_items: self.playlists.b_items.clone(),
            },
            strobe: self.strobe,
            lfo_slots: self.lfo_engine.slots,
        }
    }

    /// Puts a captured session back. Playlists come back stopped and the
    /// recall/timeline start idle: a restart should never begin by
    /// advancing presets or replaying a timeline on its own.
    pub fn apply_session(&mut self, session: &Session) {
        let set = &session.shared;
        self.deck_bus = set.deck_bus;
        self.crossfader = set.crossfader;
        self.color_params_a = set.color_params_a;
        self.color_params_b = set.color_params_b;
        self.slot_composites = set.slot_composites;
        self.time_params = set.time_params;
        self.q_var_params = set.q_var_params;
        self.snapshot_slots = set.snapshots.clone();
        self.snapshot_recall_duration_sec = set.snapshot_recall_duration;
        self.timeline_keyframes = set.timeline_keyframes.clone();
        self.overlay_store.overlays = set.overlays.clone();
        self.overlay_store.queue_enabled = set.overlay_queue_enabled;
        self.overlay_store.queue_trigger = set.overlay_queue_trigger;
        self.beat_trigger_a = set.beat_trigger_a;
        self.beat_trigger_b = set.beat_trigger_b;
        self.beat_sync_a = set.beat_sync_a;
        self.beat_sync_b = set.beat_sync_b;

        self.selected_slot = session.selected_slot.min(self.deck_bus.len() - 1);
        self.lock_a = session.lock_a;
        self.lock_b = session.lock_b;
        self.strobe = session.strobe;
        self.lfo_engine.slots = session.lfo_slots;

        let playlists = &mut self.playlists;
        playlists.interval_sec = session.playlists.interval_sec;
        playlists.mode = session.playlists.mode;
        for (deck, items) in [(Deck::A, &session.playlists.a_items), (Deck::B, &session.playlists.b_items)] {
            let existing = match deck {
                Deck::A => playlists.a_items.clone(),
                Deck::B => playlists.b_items.clone(),
            };
            for name in existing {
                playlists.remove_from_playlist(deck, &name);
            }
            for name in items {
                playlists.add_to_playlist(deck, name.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::CommandId;
    use crate::lfo::LfoShape;
    use crate::show::DeckBus;

    fn names() -> [String; 4] {
        ["a".to_string(), "b".to_string(), String::new(), "d".to_string()]
    }

    #[test]
    fn a_captured_session_restores_onto_a_fresh_show() {
        let mut live = Show::default();
        live.crossfader = 0.25;
        live.deck_bus = [DeckBus::B, DeckBus::A, DeckBus::Off, DeckBus::A];
        live.selected_slot = 2;
        live.lock_b = true;
        live.strobe.enabled = true;
        live.strobe.rate = 4.0;
        live.lfo_engine.slots[1] = LfoSlot {
            enabled: true,
            shape: LfoShape::Saw,
            rate: 2.0,
            offset: 0.25,
            center: 0.4,
            amount: 0.3,
            target: Some(CommandId::Crossfader),
        };
        live.playlists.interval_sec = 42.0;
        live.playlists.mode = PlaylistMode::Shuffle;
        live.playlists.add_to_playlist(Deck::A, "one".to_string());
        live.playlists.add_to_playlist(Deck::A, "two".to_string());
        live.playlists.add_to_playlist(Deck::B, "three".to_string());

        let session = live.capture_session(names(), 1.5);

        let mut restored = Show::default();
        restored.playlists.add_to_playlist(Deck::A, "stale".to_string());
        restored.apply_session(&session);

        assert_eq!(restored.crossfader, 0.25);
        assert_eq!(restored.deck_bus, [DeckBus::B, DeckBus::A, DeckBus::Off, DeckBus::A]);
        assert_eq!(restored.selected_slot, 2);
        assert!(restored.lock_b && !restored.lock_a);
        assert!(restored.strobe.enabled);
        assert_eq!(restored.strobe.rate, 4.0);
        assert_eq!(restored.lfo_engine.slots[1], live.lfo_engine.slots[1]);
        assert_eq!(restored.playlists.interval_sec, 42.0);
        assert_eq!(restored.playlists.mode, PlaylistMode::Shuffle);
        assert_eq!(restored.playlists.a_items, vec!["one".to_string(), "two".to_string()]);
        assert_eq!(restored.playlists.b_items, vec!["three".to_string()]);
        assert!(!restored.playlists.a_playing && !restored.playlists.b_playing);
        assert_eq!(session.deck_presets, names());
        assert_eq!(session.shared.transition_time, 1.5);
    }

    #[test]
    fn an_out_of_range_selected_slot_is_clamped() {
        let mut session = Show::default().capture_session(names(), 0.0);
        session.selected_slot = 99;
        let mut show = Show::default();
        show.apply_session(&session);
        assert_eq!(show.selected_slot, 3);
    }
}
