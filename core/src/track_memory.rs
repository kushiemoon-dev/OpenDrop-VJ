//! Remembers which presets were played on which track, so the next time the
//! track comes up they can be suggested again (or recalled automatically,
//! if the user asks for that). Pure data: `app` feeds in the track changes
//! reported by rekordbox and the presets that land, and persists the result.

use std::collections::{BTreeMap, VecDeque};

/// Tracks remembered; the least recently seen one is dropped beyond this.
pub const MAX_TRACKS: usize = 500;
/// Presets remembered per track.
pub const MAX_PRESETS_PER_TRACK: usize = 20;

#[derive(Debug, Clone, Default)]
pub struct TrackMemory {
    presets: BTreeMap<String, Vec<String>>,
    /// Track keys, least recently seen first.
    order: VecDeque<String>,
    current: Option<String>,
}

impl TrackMemory {
    /// Rebuilds a memory from what was saved: the presets per track and the
    /// tracks from least to most recently seen. Tracks missing from `order`
    /// (a file from before the order was kept) count as the oldest, and the
    /// caps are applied, since a file may have been edited.
    pub fn from_parts(mut presets: BTreeMap<String, Vec<String>>, order: Vec<String>) -> Self {
        let mut ordered: VecDeque<String> = VecDeque::new();
        let unlisted: Vec<String> = presets.keys().filter(|k| !order.contains(k)).cloned().collect();
        for key in unlisted.into_iter().chain(order) {
            if presets.contains_key(&key) && !ordered.contains(&key) {
                ordered.push_back(key);
            }
        }
        while ordered.len() > MAX_TRACKS {
            if let Some(oldest) = ordered.pop_front() {
                presets.remove(&oldest);
            }
        }
        for list in presets.values_mut() {
            list.truncate(MAX_PRESETS_PER_TRACK);
        }
        Self { presets, order: ordered, current: None }
    }

    /// Track keys from least to most recently seen, for saving.
    pub fn order(&self) -> impl Iterator<Item = &String> {
        self.order.iter()
    }

    pub fn presets_map(&self) -> &BTreeMap<String, Vec<String>> {
        &self.presets
    }

    /// The key a track is stored under: artist and title, trimmed and
    /// lowercased. `None` when both are empty.
    pub fn key(artist: &str, title: &str) -> Option<String> {
        let (artist, title) = (artist.trim().to_lowercase(), title.trim().to_lowercase());
        if artist.is_empty() && title.is_empty() {
            None
        } else {
            Some(format!("{artist} - {title}"))
        }
    }

    pub fn current(&self) -> Option<&str> {
        self.current.as_deref()
    }

    /// Marks `key` as the track now playing. Returns the presets remembered
    /// for it, newest first.
    pub fn set_current_track(&mut self, key: Option<String>) -> &[String] {
        self.current = key;
        if let Some(key) = &self.current {
            self.touch(key.clone());
        }
        self.suggestions()
    }

    /// Presets remembered for the current track, newest first.
    pub fn suggestions(&self) -> &[String] {
        self.current.as_ref().and_then(|key| self.presets.get(key)).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Records that `preset` is playing on the current track. Nothing
    /// happens while no track is known.
    pub fn record_preset(&mut self, preset: &str) {
        let Some(key) = self.current.clone() else { return };
        let list = self.presets.entry(key.clone()).or_default();
        list.retain(|p| p != preset);
        list.insert(0, preset.to_string());
        list.truncate(MAX_PRESETS_PER_TRACK);
        self.touch(key);
    }

    fn touch(&mut self, key: String) {
        self.order.retain(|k| k != &key);
        self.order.push_back(key);
        while self.order.len() > MAX_TRACKS {
            if let Some(oldest) = self.order.pop_front() {
                self.presets.remove(&oldest);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(artist: &str, title: &str) -> Option<String> {
        TrackMemory::key(artist, title)
    }

    #[test]
    fn keys_ignore_case_and_padding_and_reject_blank_tracks() {
        assert_eq!(key(" Daft Punk ", "Around The World"), key("daft punk", "around the world"));
        assert_eq!(key("", ""), None);
        assert!(key("", "Untitled").is_some());
    }

    #[test]
    fn presets_played_on_a_track_come_back_newest_first_when_it_plays_again() {
        let mut memory = TrackMemory::default();
        memory.set_current_track(key("A", "Song"));
        memory.record_preset("one");
        memory.record_preset("two");
        memory.record_preset("one");

        memory.set_current_track(key("B", "Other"));
        assert!(memory.suggestions().is_empty());
        memory.record_preset("three");

        let back = memory.set_current_track(key("A", "Song"));
        assert_eq!(back, ["one", "two"]);
    }

    #[test]
    fn nothing_is_recorded_without_a_current_track() {
        let mut memory = TrackMemory::default();
        memory.record_preset("lonely");
        assert!(memory.presets_map().is_empty());
        memory.set_current_track(None);
        memory.record_preset("still lonely");
        assert!(memory.presets_map().is_empty());
    }

    #[test]
    fn each_track_keeps_a_bounded_list() {
        let mut memory = TrackMemory::default();
        memory.set_current_track(key("A", "Song"));
        for i in 0..(MAX_PRESETS_PER_TRACK + 5) {
            memory.record_preset(&format!("p{i}"));
        }
        assert_eq!(memory.suggestions().len(), MAX_PRESETS_PER_TRACK);
        assert_eq!(memory.suggestions()[0], format!("p{}", MAX_PRESETS_PER_TRACK + 4));
    }

    #[test]
    fn the_least_recently_seen_track_is_forgotten_first() {
        let mut memory = TrackMemory::default();
        for i in 0..MAX_TRACKS {
            memory.set_current_track(Some(format!("track {i}")));
            memory.record_preset("p");
        }
        // Seeing the oldest again makes it the freshest.
        memory.set_current_track(Some("track 0".to_string()));
        memory.set_current_track(Some("a new track".to_string()));
        memory.record_preset("p");

        assert!(memory.presets_map().contains_key("track 0"));
        assert!(!memory.presets_map().contains_key("track 1"));
        assert_eq!(memory.presets_map().len(), MAX_TRACKS);
    }

    #[test]
    fn the_recency_order_survives_a_save_and_a_restore() {
        let mut memory = TrackMemory::default();
        for name in ["zebra", "mango", "alpha"] {
            memory.set_current_track(Some(name.to_string()));
            memory.record_preset("p");
        }
        let saved_order: Vec<String> = memory.order().cloned().collect();
        assert_eq!(saved_order, ["zebra", "mango", "alpha"], "oldest first, not alphabetical");

        let restored = TrackMemory::from_parts(memory.presets_map().clone(), saved_order);
        assert_eq!(restored.order().cloned().collect::<Vec<_>>(), ["zebra", "mango", "alpha"]);
    }

    #[test]
    fn restoring_applies_the_caps_and_ignores_unknown_order_entries() {
        let mut presets: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for i in 0..(MAX_TRACKS + 3) {
            presets.insert(format!("track {i:04}"), (0..(MAX_PRESETS_PER_TRACK + 5)).map(|p| format!("p{p}")).collect());
        }
        let restored = TrackMemory::from_parts(presets, vec!["not a track".to_string()]);
        assert_eq!(restored.presets_map().len(), MAX_TRACKS);
        assert!(restored.presets_map().values().all(|l| l.len() == MAX_PRESETS_PER_TRACK));
        assert!(!restored.presets_map().contains_key("track 0000"), "the oldest are dropped first");
    }

    #[test]
    fn a_restored_memory_can_suggest_for_a_known_track() {
        let stored = BTreeMap::from([("a - song".to_string(), vec!["x".to_string()])]);
        let mut memory = TrackMemory::from_parts(stored, Vec::new());
        assert_eq!(memory.set_current_track(key("A", "Song")), ["x"]);
    }
}
