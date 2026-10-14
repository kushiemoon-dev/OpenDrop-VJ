//! Personal tags and notes attached to presets, and the list of presets
//! played recently. Both are keyed by preset name, the same key the catalog
//! and the favorites use, and both are pure data: `app` persists them.

use std::collections::{BTreeMap, VecDeque};

use crate::preset_index::PresetMeta;

/// Presets remembered in the history.
pub const HISTORY_CAP: usize = 50;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PresetAnnotations {
    tags: BTreeMap<String, Vec<String>>,
    notes: BTreeMap<String, String>,
    /// Bumped on every change, so a cached search can tell it is stale.
    revision: u64,
}

impl PresetAnnotations {
    pub fn from_parts(tags: BTreeMap<String, Vec<String>>, notes: BTreeMap<String, String>) -> Self {
        Self { tags, notes, revision: 0 }
    }

    pub fn tags_map(&self) -> &BTreeMap<String, Vec<String>> {
        &self.tags
    }

    pub fn notes_map(&self) -> &BTreeMap<String, String> {
        &self.notes
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn tags_of(&self, preset: &str) -> &[String] {
        self.tags.get(preset).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn note_of(&self, preset: &str) -> &str {
        self.notes.get(preset).map(String::as_str).unwrap_or("")
    }

    /// Replaces the tags from a comma separated list: lowercased, trimmed,
    /// de-duplicated, empty entries dropped. An empty result removes the
    /// entry.
    pub fn set_tags(&mut self, preset: &str, comma_separated: &str) {
        let mut tags: Vec<String> = Vec::new();
        for tag in comma_separated.split(',') {
            let tag = tag.trim().to_lowercase();
            if !tag.is_empty() && !tags.contains(&tag) {
                tags.push(tag);
            }
        }
        let changed = if tags.is_empty() {
            self.tags.remove(preset).is_some()
        } else {
            self.tags.insert(preset.to_string(), tags.clone()).as_ref() != Some(&tags)
        };
        if changed {
            self.revision += 1;
        }
    }

    pub fn set_note(&mut self, preset: &str, note: &str) {
        let note = note.trim();
        let changed = if note.is_empty() {
            self.notes.remove(preset).is_some()
        } else {
            self.notes.insert(preset.to_string(), note.to_string()).as_deref() != Some(note)
        };
        if changed {
            self.revision += 1;
        }
    }

    /// Whether `query` (already lowercased) matches the preset's name, one of
    /// its tags or its note.
    pub fn matches(&self, preset: &PresetMeta, query_lower: &str) -> bool {
        preset.name.to_lowercase().contains(query_lower)
            || self.tags_of(&preset.name).iter().any(|t| t.contains(query_lower))
            || self.note_of(&preset.name).to_lowercase().contains(query_lower)
    }
}

/// Like `preset_index::search`, but a query also matches tags and notes.
pub fn search_annotated<'a>(
    list: &'a [PresetMeta],
    query: &str,
    annotations: &PresetAnnotations,
) -> Vec<&'a PresetMeta> {
    if query.is_empty() {
        return list.iter().collect();
    }
    let q = query.to_lowercase();
    list.iter().filter(|p| annotations.matches(p, &q)).collect()
}

/// Presets played, newest first, without immediate repeats.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PresetHistory {
    entries: VecDeque<String>,
}

impl PresetHistory {
    pub fn from_names(names: impl IntoIterator<Item = String>) -> Self {
        let mut entries: VecDeque<String> = names.into_iter().collect();
        entries.truncate(HISTORY_CAP);
        Self { entries }
    }

    pub fn record(&mut self, preset: &str) {
        if self.entries.front().is_some_and(|last| last == preset) {
            return;
        }
        self.entries.push_front(preset.to_string());
        self.entries.truncate(HISTORY_CAP);
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(String::as_str)
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(name: &str) -> PresetMeta {
        PresetMeta { name: name.to_string(), category: "Other".to_string() }
    }

    #[test]
    fn tags_are_normalised_and_deduplicated() {
        let mut a = PresetAnnotations::default();
        a.set_tags("Alpha", " Calm , calm,, BASS ,");
        assert_eq!(a.tags_of("Alpha"), ["calm", "bass"]);
        a.set_tags("Alpha", "");
        assert!(a.tags_of("Alpha").is_empty());
        assert!(a.tags_map().is_empty());
    }

    #[test]
    fn the_revision_moves_only_on_real_changes() {
        let mut a = PresetAnnotations::default();
        a.set_tags("A", "x");
        let after_first = a.revision();
        a.set_tags("A", "x");
        a.set_note("A", "   ");
        assert_eq!(a.revision(), after_first);
        a.set_note("A", "good opener");
        assert!(a.revision() > after_first);
    }

    #[test]
    fn search_matches_names_tags_and_notes() {
        let presets = [meta("Alpha Swirl"), meta("Beta Pulse"), meta("Gamma")];
        let mut a = PresetAnnotations::default();
        a.set_tags("Beta Pulse", "techno, dark");
        a.set_note("Gamma", "Closing track vibe");

        let names = |q: &str| -> Vec<&str> { search_annotated(&presets, q, &a).iter().map(|p| p.name.as_str()).collect() };

        assert_eq!(names(""), ["Alpha Swirl", "Beta Pulse", "Gamma"]);
        assert_eq!(names("alpha"), ["Alpha Swirl"]);
        assert_eq!(names("TECHNO"), ["Beta Pulse"]);
        assert_eq!(names("closing"), ["Gamma"]);
        assert!(names("nothing").is_empty());
    }

    #[test]
    fn history_is_newest_first_without_immediate_repeats_and_capped() {
        let mut h = PresetHistory::default();
        h.record("a");
        h.record("b");
        h.record("b");
        h.record("a");
        assert_eq!(h.names().collect::<Vec<_>>(), ["a", "b", "a"]);

        for i in 0..(HISTORY_CAP + 10) {
            h.record(&format!("p{i}"));
        }
        assert_eq!(h.names().count(), HISTORY_CAP);
        assert_eq!(h.names().next(), Some(format!("p{}", HISTORY_CAP + 9).as_str()));
    }
}
