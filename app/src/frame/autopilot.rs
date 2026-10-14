use opendrop_core::commands::Deck;
use opendrop_core::preset_analysis::ScanTable;

use crate::preset_load::request_preset_load;
use crate::AppState;

/// How many of the most recently played presets the auto-pilot steers around.
const RECENT_TO_AVOID: usize = 12;

/// Scanned presets that rendered something usable, with their energy score.
fn usable_candidates<'a>(table: &ScanTable, names: impl Iterator<Item = &'a String>) -> Vec<(String, f32)> {
    names
        .filter(|name| !table.is_flagged(name))
        .filter_map(|name| table.energy(name).map(|energy| (name.clone(), energy)))
        .collect()
}

/// Counts this tick's beats and, for each bus that is due, loads a preset
/// whose energy matches the music's. A bus with no scan data to choose from
/// is left alone.
pub(crate) fn run_autopilot(state: &mut AppState, beats: u32) {
    let (due_a, due_b) = state.autopilot.on_beats(beats);
    for (due, deck) in [(due_a, Deck::A), (due_b, Deck::B)] {
        if !due {
            continue;
        }
        let Some(slot) = state.show.deck_bus_slot_for(deck) else { continue };
        let candidates = state.scan.with_table(|table| usable_candidates(table, state.path_by_name.keys()));
        let borrowed: Vec<(&str, f32)> = candidates.iter().map(|(name, energy)| (name.as_str(), *energy)).collect();
        let recent: Vec<&str> = state.preset_history.names().take(RECENT_TO_AVOID).collect();
        let target = state.energy_tracker.level();
        if let Some(name) = state.autopilot.choose(&borrowed, target, &recent) {
            request_preset_load(state, slot, name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opendrop_core::preset_analysis::PresetAnalysis;

    #[test]
    fn only_scanned_unflagged_presets_with_an_energy_are_candidates() {
        let mut table = ScanTable::default();
        table.record("lively", Some(PresetAnalysis { mean_luma: 100.0, motion: 12.0 }));
        table.record("black", Some(PresetAnalysis { mean_luma: 0.5, motion: 9.0 }));
        table.record("rejected", None);
        let names: Vec<String> = ["lively", "black", "rejected", "unscanned"].iter().map(|s| s.to_string()).collect();

        let candidates = usable_candidates(&table, names.iter());

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].0, "lively");
        assert!(candidates[0].1 > 0.5);
    }
}
