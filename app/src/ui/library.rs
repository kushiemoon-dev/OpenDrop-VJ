//! Library panel: scans every preset in the background to find the ones that
//! render black, never move or are very dim, and optionally hides them from
//! the browser. Nothing is deleted and a hidden preset can always be loaded
//! by name; the results also tell the energy auto-pilot how lively each
//! preset is.
//!
//! Takes individual fields, not `&mut AppState`, like the other panels.

use std::collections::HashMap;
use std::path::PathBuf;

use opendrop_core::autopilot::{Autopilot, MAX_INTERVAL_BEATS, MIN_INTERVAL_BEATS};

use crate::preset_scan::{scan_file_path, scan_scratch_dir, ScanHandle};
use crate::ui::widgets;

/// Children run side by side; the scan is heavy on the CPU and GPU, so keep
/// it well below the number of cores.
fn worker_count() -> usize {
    std::thread::available_parallelism().map_or(1, |n| (n.get() / 2).clamp(1, 4))
}

pub fn show(
    ui: &mut egui::Ui,
    scan: &ScanHandle,
    hide_flagged: &mut bool,
    path_by_name: &HashMap<String, PathBuf>,
    autopilot: &mut Autopilot,
    energy_level: f32,
) {
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
            match (std::env::current_exe(), scan_scratch_dir()) {
                (Ok(program), Some(scratch_dir)) => {
                    let jobs = path_by_name.iter().map(|(name, path)| (name.clone(), path.clone())).collect();
                    scan.start(jobs, worker_count(), program, scratch_dir, scan_file_path());
                }
                (Err(e), _) => eprintln!("[scan] cannot locate the executable: {e}"),
                (_, None) => eprintln!("[scan] no cache directory available, scan not started"),
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

    ui.separator();
    ui.heading("Auto-pilot");
    widgets::micro_label(
        ui,
        "Every so many beats, picks a scanned preset whose energy matches the music's. Off by default, per deck bus. While on, it replaces that bus's beat sync, volume peak and playlist timer advances.",
    );
    let usable = scanned > flagged;
    ui.add_enabled_ui(usable, |ui| {
        ui.checkbox(&mut autopilot.enabled_a, "Auto-pilot on deck bus A");
        ui.checkbox(&mut autopilot.enabled_b, "Auto-pilot on deck bus B");
        let mut beats = autopilot.interval_beats();
        ui.horizontal(|ui| {
            ui.label("Change every");
            if ui.add(egui::DragValue::new(&mut beats).range(MIN_INTERVAL_BEATS..=MAX_INTERVAL_BEATS).suffix(" beats")).changed() {
                autopilot.set_interval_beats(beats);
            }
        });
    });
    if !usable {
        widgets::micro_label(ui, "Scan the library first: the auto-pilot needs to know how lively each preset is.");
    }
    ui.horizontal(|ui| {
        ui.label("Music energy");
        ui.add(egui::ProgressBar::new(energy_level.clamp(0.0, 1.0)).desired_width(160.0));
    });
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
            let mut pilot = Autopilot::default();
            show(ui, &scan, &mut hide, &HashMap::new(), &mut pilot, 0.4);
            let paths = HashMap::from([("a".to_string(), PathBuf::from("/x/a.milk"))]);
            show(ui, &scan, &mut hide, &paths, &mut pilot, 0.4);
        });
    }

    #[test]
    fn worker_count_stays_in_a_sane_range() {
        assert!((1..=4).contains(&worker_count()));
    }
}
