//! The Log tab of the About panel: what the app reported on stderr and in
//! the log file, newest at the bottom, so a problem can be copied into a bug
//! report without a terminal.

use opendrop_core::error_log;

use crate::ui::widgets;

pub fn show(ui: &mut egui::Ui) {
    let entries = error_log::entries();
    ui.horizontal(|ui| {
        if ui.button("Copy all").clicked() {
            let text: Vec<String> = entries.iter().map(|e| error_log::format_line(e.at, &e.message)).collect();
            ui.ctx().copy_text(text.join("\n"));
        }
        if ui.button("Clear").clicked() {
            error_log::clear();
        }
    });
    let location = match error_log::file_path() {
        Some(path) => format!("Also saved to {}", path.display()),
        None => "The log file could not be located.".to_string(),
    };
    widgets::micro_label(ui, &location);
    ui.separator();

    if entries.is_empty() {
        widgets::micro_label(ui, "No errors logged.");
        return;
    }
    egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
        for entry in &entries {
            ui.add(egui::Label::new(egui::RichText::new(error_log::format_line(entry.at, &entry.message)).monospace()).wrap());
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::widgets::themed_test_ui;

    #[test]
    fn the_tab_renders_empty_and_with_entries() {
        themed_test_ui(show);
        error_log::record("[test] something failed");
        themed_test_ui(show);
    }
}
