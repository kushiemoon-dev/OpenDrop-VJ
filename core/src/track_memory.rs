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
    pub fn from_parts(presets: BTreeMap<String, Vec<String>>) -> Self {
        let order = presets.keys().cloned().collect();
        Self { presets, order, current: None }
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
    fn a_restored_memory_can_suggest_for_a_known_track() {
        let stored = BTreeMap::from([("a - song".to_string(), vec!["x".to_string()])]);
        let mut memory = TrackMemory::from_parts(stored);
        assert_eq!(memory.set_current_track(key("A", "Song")), ["x"]);
    }
}
