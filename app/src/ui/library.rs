//! Library panel: scans every preset in the background to find the ones that
//! render black, never move or are very dim, and optionally hides them from
//! the browser. Nothing is deleted and a hidden preset can always be loaded
//! by name; the results also tell the energy auto-pilot how lively each
//! preset is.
//!
//! Takes individual fields, not `&mut AppState`, like the other panels.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::preset_scan::{scan_file_path, ScanHandle};
use crate::ui::widgets;

/// Children run side by side; the scan is heavy on the CPU and GPU, so keep
/// it well below the number of cores.
fn worker_count() -> usize {
    std::thread::available_parallelism().map_or(1, |n| (n.get() / 2).clamp(1, 4))
}

pub fn show(ui: &mut egui::Ui, scan: &ScanHandle, hide_flagged: &mut bool, path_by_name: &HashMap<String, PathBuf>) {
    ui.heading("Library scan");
    widgets::micro_label(
        ui,
        "Renders each preset for a moment to spot black, frozen and very dim ones. Best run before a set: it uses the CPU and GPU.",
    );

    let progress = scan.progress();
    let (scanned, flagged) = scan.with_table(|t| (t.len(), t.flagged_count()));

    ui.horizontal(|ui| {
        if progress.running {
            if ui.button("Cancel").clicked() {
                scan.cancel();
            }
            ui.label(format!("Scanning {} of {}", progress.done, progress.total));
        } else if ui.add_enabled(!path_by_name.is_empty(), egui::Button::new("Scan library")).clicked() {
            match std::env::current_exe() {
                Ok(program) => {
                    let jobs = path_by_name.iter().map(|(name, path)| (name.clone(), path.clone())).collect();
                    scan.start(jobs, worker_count(), program, scan_file_path());
                }
                Err(e) => eprintln!("[scan] cannot locate the executable: {e}"),
            }
        }
    });
    if progress.running && progress.total > 0 {
        ui.add(egui::ProgressBar::new(progress.done as f32 / progress.total as f32));
    }

    ui.label(format!("{scanned} of {} presets scanned, {flagged} flagged.", path_by_name.len()));

    ui.separator();
    ui.checkbox(hide_flagged, "Hide flagged presets in the browser");
    widgets::micro_label(ui, "Flagged: black, frozen, very dim, or rejected by the renderer.");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::widgets::themed_test_ui;
    use opendrop_core::preset_analysis::ScanTable;

    #[test]
    fn show_does_not_panic_empty_and_with_results() {
        themed_test_ui(|ui| {
            let mut table = ScanTable::default();
            table.record("a", None);
            let scan = ScanHandle::new(table);
            let mut hide = false;
            show(ui, &scan, &mut hide, &HashMap::new());
            let paths = HashMap::from([("a".to_string(), PathBuf::from("/x/a.milk"))]);
            show(ui, &scan, &mut hide, &paths);
        });
    }

    #[test]
    fn worker_count_stays_in_a_sane_range() {
        assert!((1..=4).contains(&worker_count()));
    }
}
