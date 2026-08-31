//! Preset metadata indexing and search: pure logic layer for preset catalog.
//!
//! Handles preset categorization (parsing " - " delimiters) and case-insensitive
//! substring search over the preset name field. No I/O; disk scanning stays in the `app` crate.

use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresetMeta {
    pub name: String,
    pub category: String,
}

/// Extract category from preset name using " - " delimiter.
/// If " - " is found at position i > 0, returns the substring before it (trimmed).
/// If delimiter is at position 0 or not found, returns "Other".
pub fn category_from_name(name: &str) -> String {
    match name.find(" - ") {
        Some(i) if i > 0 => name[..i].trim().to_string(),
        _ => "Other".to_string(),
    }
}

/// Search preset list by case-insensitive substring match on preset name.
/// Returns references to all matching presets, in original order.
/// Empty query returns all presets.
pub fn search<'a>(list: &'a [PresetMeta], query: &str) -> Vec<&'a PresetMeta> {
    if query.is_empty() {
        return list.iter().collect();
    }
    let q = query.to_lowercase();
    list
        .iter()
        .filter(|p| p.name.to_lowercase().contains(&q))
        .collect()
}

/// Filters an already-searched preset list down to only the names present
/// in `favorites`. Composed after `search()` rather than added as a
/// parameter to it, so `search()`'s existing contract/tests stay untouched.
pub fn filter_favorites<'a>(list: Vec<&'a PresetMeta>, favorites: &HashSet<String>) -> Vec<&'a PresetMeta> {
    list.into_iter().filter(|p| favorites.contains(&p.name)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    mod category_from_name {
        use super::*;

        #[test]
        fn with_valid_delimiter_extracts_category() {
            let name = "Psychedelic - Swirly Loops";
            let result = category_from_name(name);
            assert_eq!(result, "Psychedelic");
        }

        #[test]
        fn with_valid_delimiter_trims_whitespace() {
            let name = "  Wave   - Shimmer";
            let result = category_from_name(name);
            assert_eq!(result, "Wave");
        }

        #[test]
        fn delimiter_at_position_zero_returns_other() {
            let name = " - SomethingElse";
            let result = category_from_name(name);
            assert_eq!(result, "Other");
        }

        #[test]
        fn without_delimiter_returns_other() {
            let name = "Random Preset Name";
            let result = category_from_name(name);
            assert_eq!(result, "Other");
        }

        #[test]
        fn empty_string_returns_other() {
            let name = "";
            let result = category_from_name(name);
            assert_eq!(result, "Other");
        }
    }

    mod search {
        use super::*;

        #[test]
        fn empty_query_returns_all() {
            let presets = vec![
                PresetMeta {
                    name: "Psychedelic - Swirly Loops".to_string(),
                    category: "Psychedelic".to_string(),
                },
                PresetMeta {
                    name: "Wave - Shimmer".to_string(),
                    category: "Wave".to_string(),
                },
                PresetMeta {
                    name: "Glitch - Chaos".to_string(),
                    category: "Glitch".to_string(),
                },
            ];

            let results = search(&presets, "");
            assert_eq!(results.len(), 3);
        }

        #[test]
        fn substring_match_found() {
            let presets = vec![
                PresetMeta {
                    name: "Psychedelic - Swirly Loops".to_string(),
                    category: "Psychedelic".to_string(),
                },
                PresetMeta {
                    name: "Wave - Shimmer".to_string(),
                    category: "Wave".to_string(),
                },
            ];

            let results = search(&presets, "Swirl");
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].name, "Psychedelic - Swirly Loops");
        }

        #[test]
        fn substring_match_not_found() {
            let presets = vec![
                PresetMeta {
                    name: "Psychedelic - Swirly Loops".to_string(),
                    category: "Psychedelic".to_string(),
                },
                PresetMeta {
                    name: "Wave - Shimmer".to_string(),
                    category: "Wave".to_string(),
                },
            ];

            let results = search(&presets, "Laser");
            assert_eq!(results.len(), 0);
        }

        #[test]
        fn search_is_case_insensitive() {
            let presets = vec![
                PresetMeta {
                    name: "Psychedelic - Swirly Loops".to_string(),
                    category: "Psychedelic".to_string(),
                },
                PresetMeta {
                    name: "Wave - Shimmer".to_string(),
                    category: "Wave".to_string(),
                },
            ];

            let results = search(&presets, "SWIRL");
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].name, "Psychedelic - Swirly Loops");
        }

        #[test]
        fn search_matches_partial_in_category() {
            let presets = vec![
                PresetMeta {
                    name: "Psychedelic - Swirly Loops".to_string(),
                    category: "Psychedelic".to_string(),
                },
                PresetMeta {
                    name: "Wave - Shimmer".to_string(),
                    category: "Wave".to_string(),
                },
            ];

            let results = search(&presets, "psyche");
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].name, "Psychedelic - Swirly Loops");
        }

        #[test]
        fn search_multiple_matches() {
            let presets = vec![
                PresetMeta {
                    name: "Psychedelic - Swirly Loops".to_string(),
                    category: "Psychedelic".to_string(),
                },
                PresetMeta {
                    name: "Psycho - Killer".to_string(),
                    category: "Psycho".to_string(),
                },
                PresetMeta {
                    name: "Wave - Shimmer".to_string(),
                    category: "Wave".to_string(),
                },
            ];

            let results = search(&presets, "Psych");
            assert_eq!(results.len(), 2);
        }

        #[test]
        fn search_empty_list() {
            let presets: Vec<PresetMeta> = vec![];

            let results = search(&presets, "anything");
            assert_eq!(results.len(), 0);
        }
    }

    mod filter_favorites {
        use super::*;

        #[test]
        fn empty_favorites_returns_empty_list() {
            let a = PresetMeta { name: "Psychedelic - Swirly Loops".to_string(), category: "Psychedelic".to_string() };
            let b = PresetMeta { name: "Wave - Shimmer".to_string(), category: "Wave".to_string() };
            let list = vec![&a, &b];

            let results = filter_favorites(list, &HashSet::new());
            assert_eq!(results.len(), 0);
        }

        #[test]
        fn matching_subset_is_returned_intact() {
            let a = PresetMeta { name: "Psychedelic - Swirly Loops".to_string(), category: "Psychedelic".to_string() };
            let b = PresetMeta { name: "Wave - Shimmer".to_string(), category: "Wave".to_string() };
            let list = vec![&a, &b];
            let favorites = HashSet::from(["Psychedelic - Swirly Loops".to_string()]);

            let results = filter_favorites(list, &favorites);
            assert_eq!(results, vec![&a]);
        }

        #[test]
        fn name_absent_from_favorites_is_excluded() {
            let a = PresetMeta { name: "Psychedelic - Swirly Loops".to_string(), category: "Psychedelic".to_string() };
            let list = vec![&a];
            let favorites = HashSet::from(["Wave - Shimmer".to_string()]);

            let results = filter_favorites(list, &favorites);
            assert_eq!(results.len(), 0);
        }

        #[test]
        fn empty_input_list_returns_empty_list() {
            let list: Vec<&PresetMeta> = vec![];
            let favorites = HashSet::from(["Wave - Shimmer".to_string()]);

            let results = filter_favorites(list, &favorites);
            assert_eq!(results.len(), 0);
        }
    }
}
