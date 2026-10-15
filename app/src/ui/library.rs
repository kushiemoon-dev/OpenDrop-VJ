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

/// Where the presets and the textures they use live. Saved as soon as one is
/// changed; the new folders are read at the next launch.
fn folders_section(ui: &mut egui::Ui, preset_dir: &mut String, texture_dir: &mut String) {
    ui.heading("Folders");
    let mut changed = false;
    changed |= folder_row(ui, "Presets", preset_dir);
    changed |= folder_row(ui, "Textures", texture_dir);
    if changed {
        persist_folders(preset_dir, texture_dir);
    }
    widgets::micro_label(
        ui,
        "Restart OpenDrop to use a new folder. Blank uses the default. OPENDROP_PRESET_DIR and OPENDROP_TEXTURE_DIR override these.",
    );
}

/// One label, a text box, Browse and Clear. Returns whether the value changed.
fn folder_row(ui: &mut egui::Ui, label: &str, value: &mut String) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        let response = ui.add(egui::TextEdit::singleline(value).desired_width(260.0));
        changed |= response.lost_focus() && response.changed();
        if ui.button("Browse").clicked() {
            if let Some(folder) = rfd::FileDialog::new().set_title(format!("{label} folder")).pick_folder() {
                *value = folder.to_string_lossy().into_owned();
                changed = true;
            }
        }
        if ui.add_enabled(!value.is_empty(), egui::Button::new("Clear")).clicked() {
            value.clear();
            changed = true;
        }
    });
    changed
}

/// Read-modify-write of just the two folder fields, so nothing else saved in
/// `ui.json` is touched.
fn persist_folders(preset_dir: &str, texture_dir: &str) {
    let path = crate::config::config_file_path();
    let mut config = path.as_deref().map(crate::config::load_config).unwrap_or_default();
    config.preset_dir = crate::ui_persist::non_empty(preset_dir);
    config.texture_dir = crate::ui_persist::non_empty(texture_dir);
    crate::config::save_config(path.as_deref(), &config);
}

#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    scan: &ScanHandle,
    hide_flagged: &mut bool,
    preset_dir: &mut String,
    texture_dir: &mut String,
    path_by_name: &HashMap<String, PathBuf>,
    autopilot: &mut Autopilot,
    energy_level: f32,
) {
    folders_section(ui, preset_dir, texture_dir);
    ui.separator();

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
                (Err(e), _) => opendrop_core::log_error!("[scan] cannot locate the executable: {e}"),
                (_, None) => opendrop_core::log_error!("[scan] no cache directory available, scan not started"),
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
            let (mut presets, mut textures) = (String::new(), String::new());
            show(ui, &scan, &mut hide, &mut presets, &mut textures, &HashMap::new(), &mut pilot, 0.4);
            let paths = HashMap::from([("a".to_string(), PathBuf::from("/x/a.milk"))]);
            show(ui, &scan, &mut hide, &mut presets, &mut textures, &paths, &mut pilot, 0.4);
        });
    }

    #[test]
    fn worker_count_stays_in_a_sane_range() {
        assert!((1..=4).contains(&worker_count()));
    }
}
