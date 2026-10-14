//! Preset notes panel: personal tags and a note for the preset loaded on the
//! selected deck, plus the list of presets played recently, a click on one
//! reloading it. Tags and notes are searched by the preset browser.
//!
//! Takes individual fields, not `&mut AppState`, like the other panels.
//! `AnnotationEdit` holds the text being typed, so a half-typed tag list is
//! not normalised under the cursor; every change is written through to the
//! annotations straight away.

use opendrop_core::preset_annotations::{PresetAnnotations, PresetHistory};
use opendrop_core::track_memory::TrackMemory;

use crate::ui::widgets;

/// How many recent presets are listed.
const RECENT_SHOWN: usize = 15;

/// The text fields for one preset, reloaded when the selection changes.
#[derive(Default)]
pub struct AnnotationEdit {
    preset: String,
    tags: String,
    note: String,
}

impl AnnotationEdit {
    /// Reloads the buffers from the stored annotations when `preset` is not
    /// the one they were last loaded for.
    fn sync(&mut self, preset: &str, annotations: &PresetAnnotations) {
        if self.preset != preset {
            self.preset = preset.to_string();
            self.tags = annotations.tags_of(preset).join(", ");
            self.note = annotations.note_of(preset).to_string();
        }
    }
}

/// What the panel needs to follow the current track.
pub struct TrackSettings<'a> {
    pub memory: &'a TrackMemory,
    pub auto_recall: &'a mut bool,
}

pub fn show(
    ui: &mut egui::Ui,
    current_preset: &str,
    annotations: &mut PresetAnnotations,
    history: &PresetHistory,
    edit: &mut AnnotationEdit,
    track: TrackSettings<'_>,
    load_request: &mut Option<String>,
) {
    edit.sync(current_preset, annotations);

    ui.heading("Preset notes");
    if current_preset.is_empty() {
        widgets::micro_label(ui, "No preset on the selected deck.");
    } else {
        widgets::micro_label(ui, current_preset);

        ui.horizontal(|ui| {
            ui.label("Tags");
            if ui.text_edit_singleline(&mut edit.tags).changed() {
                annotations.set_tags(current_preset, &edit.tags);
            }
        });
        widgets::micro_label(ui, "Comma separated. The preset browser search matches tags and notes.");

        ui.horizontal(|ui| {
            ui.label("Note");
            if ui.text_edit_singleline(&mut edit.note).changed() {
                annotations.set_note(current_preset, &edit.note);
            }
        });
    }

    ui.separator();
    ui.label("Played on this track before");
    match track.memory.current() {
        None => {
            widgets::micro_label(ui, "No track reported. Connect rkbx_link to follow the track playing.");
        }
        Some(_) if track.memory.suggestions().is_empty() => {
            widgets::micro_label(ui, "Nothing remembered for this track yet.");
        }
        Some(_) => {
            for name in track.memory.suggestions().iter().take(RECENT_SHOWN) {
                if ui.selectable_label(false, name).clicked() {
                    *load_request = Some(name.clone());
                }
            }
        }
    }
    ui.checkbox(track.auto_recall, "Reload the last preset automatically when a known track starts");

    ui.separator();
    ui.label("Recently played");
    let mut any = false;
    for name in history.names().take(RECENT_SHOWN) {
        any = true;
        if ui.selectable_label(false, name).clicked() {
            *load_request = Some(name.to_string());
        }
    }
    if !any {
        widgets::micro_label(ui, "Nothing played yet.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::widgets::themed_test_ui;

    #[test]
    fn switching_preset_reloads_the_buffers_from_the_stored_values() {
        let mut annotations = PresetAnnotations::default();
        annotations.set_tags("A", "calm, slow");
        annotations.set_note("A", "opener");
        let mut edit = AnnotationEdit::default();

        edit.sync("A", &annotations);
        assert_eq!((edit.tags.as_str(), edit.note.as_str()), ("calm, slow", "opener"));

        edit.tags = "typed but not stored yet".to_string();
        edit.sync("A", &annotations);
        assert_eq!(edit.tags, "typed but not stored yet", "same preset keeps the text being typed");

        edit.sync("B", &annotations);
        assert_eq!((edit.tags.as_str(), edit.note.as_str()), ("", ""));
    }

    #[test]
    fn show_does_not_panic_with_and_without_a_preset() {
        themed_test_ui(|ui| {
            let mut annotations = PresetAnnotations::default();
            let mut history = PresetHistory::default();
            history.record("Alpha");
            let mut edit = AnnotationEdit::default();
            let mut request = None;
            let track = TrackMemory::default();
            let mut auto_recall = false;
            show(ui, "", &mut annotations, &history, &mut edit, TrackSettings { memory: &track, auto_recall: &mut auto_recall }, &mut request);
            show(ui, "Alpha", &mut annotations, &history, &mut edit, TrackSettings { memory: &track, auto_recall: &mut auto_recall }, &mut request);
        });
    }
}
