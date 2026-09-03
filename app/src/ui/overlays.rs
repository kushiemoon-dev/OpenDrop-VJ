//! Overlays panel: sprite/text overlays drawn on top of the composited
//! decks, plus the auto-cycling queue that rotates through the ones marked
//! for it. Port of `SidebarOverlays.svelte` (Step 12 of the Phase 8
//! VJ-panels plan). The richest of the ported sidebars, and the one whose
//! engine half (`core::overlay`, ported long ago and unused until now) this
//! step finally gives a consumer.
//!
//! Same "direct field mutation, no `CommandRegistry::dispatch`" convention
//! as every other panel for its own controls, with one exception matching
//! the plan's transversal command list: the queue's ◀/▶ buttons dispatch
//! `CommandId::OverlayQueuePrev`/`Next` so a keyboard/MIDI/OSC/remote-ws
//! binding and a click take the exact same path (Recipe B: same precedent
//! as `ui::strobe`'s toggle and `ui::timeline`'s Play/Pause).
//!
//! Two pieces of state live outside `core::overlay::Overlay` because a
//! zero-I/O crate cannot own them, and this panel writes both:
//! - `assets`: overlay id → the image file it was created from. The web
//!   kept the bytes in IndexedDB keyed by the same id; here the file stays
//!   where the user picked it and `app` re-reads it on demand.
//! - `next_id`: the id counter. The web used `crypto.randomUUID()`;
//!   `core::overlay`'s ported API takes the id as a parameter precisely so
//!   the I/O-capable layer can choose (see that module's doc comment).

use std::collections::HashMap;
use std::path::PathBuf;

use opendrop_core::beat_trigger::{clamp_beats_per_change, clamp_offset, BeatTriggerConfigPatch, BeatTriggerMode};
use opendrop_core::commands::{CommandId, CommandRegistry};
use opendrop_core::overlay::{FontFamily, OverlayKind, OverlayPatch};
use opendrop_core::playlist::PlaylistMode;
use opendrop_core::show::Show;
use opendrop_engine::compositor::OverlayBlendMode;

use crate::ui::widgets::{self, theme};

/// Extensions the sprite file dialog accepts: exactly the decoders
/// `engine`'s `image` dependency is built with (`engine/Cargo.toml`).
const SPRITE_EXTENSIONS: [&str; 7] = ["png", "jpg", "jpeg", "gif", "bmp", "webp", "PNG"];

const FONT_FAMILIES: [(FontFamily, &str); 5] = [
    (FontFamily::Sans, "Sans"),
    (FontFamily::Serif, "Serif"),
    (FontFamily::Mono, "Mono"),
    (FontFamily::Impact, "Impact"),
    (FontFamily::Comic, "Comic"),
];

/// Which overlay's editor is expanded, if any (`expandedOverlayId` in the
/// Svelte source). Frame-to-frame widget state, so it goes through egui's
/// own memory rather than a new `AppState` field: this is a pure
/// disclosure toggle with no meaning outside the panel, unlike e.g.
/// `cloud_presets_rename`, which carries an edit buffer.
fn expanded_id(ui: &egui::Ui) -> Option<String> {
    ui.data(|d| d.get_temp::<String>(egui::Id::new("od_overlay_expanded")))
}

fn set_expanded_id(ui: &egui::Ui, id: Option<String>) {
    let key = egui::Id::new("od_overlay_expanded");
    match id {
        Some(id) => {
            ui.data_mut(|d| d.insert_temp(key, id));
        }
        None => ui.data_mut(|d| d.remove::<String>(key)),
    }
}

pub fn show(
    ui: &mut egui::Ui,
    show: &mut Show,
    assets: &mut HashMap<String, PathBuf>,
    next_id: &mut u64,
    registry: &CommandRegistry,
) {
    ui.horizontal(|ui| {
        ui.heading(format!("Overlays ({})", show.overlay_store.overlays.len()));
        if ui.button("+ Text").clicked() {
            let id = show.overlay_store.add_text_overlay(mint_id(next_id));
            set_expanded_id(ui, Some(id));
        }
        if ui.button("+ Sprite").clicked() {
            add_sprites_via_file_dialog(show, assets, next_id);
        }
    });

    if show.overlay_store.overlays.is_empty() {
        widgets::micro_label(ui, "No overlays yet. Add one with \"+ Sprite\" for an image or \"+ Text\" for text.");
    }

    ui.separator();

    let expanded = expanded_id(ui);
    // Collected before the loop so the body can mutate the store: an id
    // whose ✕ was clicked this frame, applied after the iteration.
    let mut to_remove: Option<String> = None;
    let mut expand_request: Option<Option<String>> = None;

    for index in 0..show.overlay_store.overlays.len() {
        let id = show.overlay_store.overlays[index].id.clone();
        ui.push_id(index, |ui| {
            let is_expanded = expanded.as_deref() == Some(id.as_str());
            ui.horizontal(|ui| {
                let t = theme(ui);
                let name = show.overlay_store.overlays[index].name.clone();
                if ui.selectable_label(is_expanded, name).clicked() {
                    expand_request = Some(if is_expanded { None } else { Some(id.clone()) });
                }
                let beat_reactive = show.overlay_store.overlays[index].beat_reactive;
                let color = if beat_reactive { t.palette.accent } else { t.palette.dim };
                if widgets::pill(ui, "♩", color).interact(egui::Sense::click()).clicked() {
                    show.overlay_store
                        .update_overlay(&id, OverlayPatch { beat_reactive: Some(!beat_reactive), ..Default::default() });
                }
                let in_queue = show.overlay_store.overlays[index].in_queue;
                let color = if in_queue { t.palette.accent } else { t.palette.dim };
                if widgets::pill(ui, "▤", color).interact(egui::Sense::click()).clicked() {
                    show.overlay_store
                        .update_overlay(&id, OverlayPatch { in_queue: Some(!in_queue), ..Default::default() });
                }
                if ui.button("✕").clicked() {
                    to_remove = Some(id.clone());
                }
            });
            if is_expanded {
                editor(ui, show, &id, index);
            }
        });
    }

    if let Some(request) = expand_request {
        set_expanded_id(ui, request);
    }
    if let Some(id) = to_remove {
        show.overlay_store.remove_overlay(&id);
        assets.remove(&id);
        if expanded.as_deref() == Some(id.as_str()) {
            set_expanded_id(ui, None);
        }
    }

    ui.separator();
    queue_controls(ui, show, registry);
}

/// The expanded per-overlay editor (`.overlay-controls` in the Svelte
/// source). Every field is read out of the store, edited through a local,
/// then written back as an `OverlayPatch`: `update_overlay` is the store's
/// only mutation path for an existing overlay, and going through it keeps
/// the patch/merge semantics identical to the TS `{ ...o, ...patch }`.
fn editor(ui: &mut egui::Ui, show: &mut Show, id: &str, index: usize) {
    let overlay = &show.overlay_store.overlays[index];
    let mut patch = OverlayPatch::default();

    if overlay.kind == OverlayKind::Text {
        widgets::micro_label(ui, "Content");
        let mut text = overlay.text.clone();
        if ui.add(egui::TextEdit::multiline(&mut text).desired_rows(2)).changed() {
            patch.text = Some(text);
        }

        widgets::micro_label(ui, "Font");
        let current_font = overlay.font_family;
        let selected = FONT_FAMILIES.iter().find(|(f, _)| *f == current_font).map_or("Sans", |(_, l)| *l);
        egui::ComboBox::from_id_salt("od_overlay_font").selected_text(selected).show_ui(ui, |ui| {
            for (family, label) in FONT_FAMILIES {
                if ui.selectable_label(current_font == family, label).clicked() && current_font != family {
                    patch.font_family = Some(family);
                }
            }
        });
        // Honest about the vendored font set: `app/assets/fonts/` carries
        // Inter and JetBrains Mono only (Phase 7 Step 2), so three of the
        // five families in the ported `FontFamily` enum have no face of
        // their own to render with. Bundling a serif/display/comic face is
        // a vendoring + licensing decision, not this step's call.
        widgets::micro_label(ui, "Serif, Impact, and Comic all render with the Sans font (no dedicated face bundled).");

        widgets::micro_label(ui, "Color");
        let mut rgb = hex_to_rgb_f32(&overlay.color);
        if ui.color_edit_button_rgb(&mut rgb).changed() {
            patch.color = Some(rgb_f32_to_hex(rgb));
        }

        widgets::micro_label(ui, "Size (vh)");
        let mut font_size = overlay.font_size;
        if ui.add(egui::Slider::new(&mut font_size, 2.0..=20.0).step_by(0.5)).changed() {
            patch.font_size = Some(font_size);
        }
    }

    widgets::micro_label(ui, "Opacity");
    let mut opacity = overlay.opacity;
    if ui.add(egui::Slider::new(&mut opacity, 0.0..=1.0).step_by(0.01)).changed() {
        patch.opacity = Some(opacity);
    }

    widgets::micro_label(ui, "Scale");
    let mut scale = overlay.scale;
    if ui.add(egui::Slider::new(&mut scale, 0.05..=4.0).step_by(0.05)).changed() {
        patch.scale = Some(scale);
    }

    widgets::micro_label(ui, "X");
    let mut x = overlay.x;
    if ui.add(egui::Slider::new(&mut x, 0.0..=1.0).step_by(0.01)).changed() {
        patch.x = Some(x);
    }

    widgets::micro_label(ui, "Y");
    let mut y = overlay.y;
    if ui.add(egui::Slider::new(&mut y, 0.0..=1.0).step_by(0.01)).changed() {
        patch.y = Some(y);
    }

    // Rotation is Media-only in the Svelte source (a text overlay's static
    // tilt was never exposed there); `spin` below is offered for both.
    if overlay.kind != OverlayKind::Text {
        widgets::micro_label(ui, "Rotation");
        let mut rotation = overlay.rotation;
        if ui.add(egui::Slider::new(&mut rotation, -180.0..=180.0).step_by(1.0)).changed() {
            patch.rotation = Some(rotation);
        }
    }

    widgets::micro_label(ui, "Spin (°/s)");
    let mut spin = overlay.spin;
    if ui.add(egui::Slider::new(&mut spin, -180.0..=180.0).step_by(1.0)).changed() {
        patch.spin = Some(spin);
    }

    widgets::micro_label(ui, "Drift X");
    let mut drift_x = overlay.drift_x;
    if ui.add(egui::Slider::new(&mut drift_x, -1.0..=1.0).step_by(0.05)).changed() {
        patch.drift_x = Some(drift_x);
    }

    widgets::micro_label(ui, "Drift Y");
    let mut drift_y = overlay.drift_y;
    if ui.add(egui::Slider::new(&mut drift_y, -1.0..=1.0).step_by(0.05)).changed() {
        patch.drift_y = Some(drift_y);
    }

    widgets::micro_label(ui, "Blend");
    let current_blend = OverlayBlendMode::from_css(&overlay.blend_mode);
    egui::ComboBox::from_id_salt("od_overlay_blend").selected_text(current_blend.as_css()).show_ui(ui, |ui| {
        for mode in OverlayBlendMode::ALL {
            if ui.selectable_label(current_blend == mode, mode.as_css()).clicked() && current_blend != mode {
                patch.blend_mode = Some(mode.as_css().to_string());
            }
        }
    });

    show.overlay_store.update_overlay(id, patch);
}

/// The two `.beat-trigger-row`s at the bottom of the Svelte panel: queue
/// transport + mode, then the beat/volume trigger configuration.
fn queue_controls(ui: &mut egui::Ui, show: &mut Show, registry: &CommandRegistry) {
    widgets::section(ui, "Auto Queue");

    ui.horizontal(|ui| {
        let label = if show.overlay_store.queue_enabled { "⏸" } else { "▶" };
        if ui.button(label).clicked() {
            show.overlay_store.toggle_overlay_queue();
        }
        if ui.button("◀").clicked() {
            registry.dispatch(CommandId::OverlayQueuePrev, 1.0, show);
        }
        if ui.button("▶|").clicked() {
            registry.dispatch(CommandId::OverlayQueueNext, 1.0, show);
        }
        let mode = show.overlay_store.queue_mode;
        egui::ComboBox::from_id_salt("od_overlay_queue_mode")
            .selected_text(match mode {
                PlaylistMode::Sequential => "Sequential",
                PlaylistMode::Shuffle => "Shuffle",
            })
            .show_ui(ui, |ui| {
                for (candidate, label) in
                    [(PlaylistMode::Sequential, "Sequential"), (PlaylistMode::Shuffle, "Shuffle")]
                {
                    if ui.selectable_label(mode == candidate, label).clicked() && mode != candidate {
                        show.overlay_store.set_overlay_queue_mode(candidate);
                    }
                }
            });
    });

    let trigger = show.overlay_store.queue_trigger;
    ui.horizontal(|ui| {
        if ui.button("÷2").clicked() {
            show.overlay_store.update_overlay_queue_trigger(BeatTriggerConfigPatch {
                beats_per_change: Some(clamp_beats_per_change(trigger.beats_per_change as i64 / 2) as i64),
                ..Default::default()
            });
        }
        let mut beats = trigger.beats_per_change as i64;
        if ui.add(egui::DragValue::new(&mut beats).range(1..=64)).changed() {
            show.overlay_store.update_overlay_queue_trigger(BeatTriggerConfigPatch {
                beats_per_change: Some(beats),
                ..Default::default()
            });
        }
        if ui.button("×2").clicked() {
            show.overlay_store.update_overlay_queue_trigger(BeatTriggerConfigPatch {
                beats_per_change: Some(clamp_beats_per_change(trigger.beats_per_change as i64 * 2) as i64),
                ..Default::default()
            });
        }
        ui.label("off");
        let mut offset = trigger.offset as i64;
        if ui.add(egui::DragValue::new(&mut offset).range(0..=(trigger.beats_per_change as i64 - 1))).changed() {
            show.overlay_store.update_overlay_queue_trigger(BeatTriggerConfigPatch {
                offset: Some(clamp_offset(offset, trigger.beats_per_change) as i64),
                ..Default::default()
            });
        }
        let t = theme(ui);
        let volume_mode = trigger.mode == BeatTriggerMode::VolumePeak;
        let color = if volume_mode { t.palette.accent } else { t.palette.dim };
        let label = if volume_mode { "🔊" } else { "♩" };
        if widgets::pill(ui, label, color).interact(egui::Sense::click()).clicked() {
            let next = if volume_mode { BeatTriggerMode::Beat } else { BeatTriggerMode::VolumePeak };
            show.overlay_store
                .update_overlay_queue_trigger(BeatTriggerConfigPatch { mode: Some(next), ..Default::default() });
        }
    });

    if trigger.mode == BeatTriggerMode::VolumePeak {
        widgets::micro_label(ui, "Sensitivity");
        let mut sensitivity = trigger.sensitivity;
        if ui.add(egui::Slider::new(&mut sensitivity, 0.0..=1.0).step_by(0.01)).changed() {
            show.overlay_store.update_overlay_queue_trigger(BeatTriggerConfigPatch {
                sensitivity: Some(sensitivity),
                ..Default::default()
            });
        }
    }
}

/// Monotonic per-session overlay id. The web's `crypto.randomUUID()` had
/// to be globally unique because overlays were persisted into IndexedDB
/// across sessions; nothing here outlives the process, so a counter is
/// enough, and it makes `assets`/`overlay_textures` lookups trivially
/// debuggable.
fn mint_id(next_id: &mut u64) -> String {
    *next_id += 1;
    format!("ov-{next_id}")
}

/// `+ Sprite`: the native stand-in for the Svelte `<input type="file"
/// multiple>`. Multi-select is preserved; each picked file becomes one
/// overlay named after the file stem, centered, exactly like
/// `addOverlayFromFile` did.
fn add_sprites_via_file_dialog(show: &mut Show, assets: &mut HashMap<String, PathBuf>, next_id: &mut u64) {
    let Some(paths) = rfd::FileDialog::new().add_filter("Images", &SPRITE_EXTENSIONS).pick_files() else {
        return; // dialog cancelled
    };
    for path in paths {
        let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "sprite".to_string());
        let id = mint_id(next_id);
        // Centered, same as `makeOverlay(name, {})`'s x/y defaults: the
        // drop-at-position path (`add_overlay_at_position`) belongs to the
        // visualizer drag-and-drop handler, not to this button.
        show.overlay_store.add_overlay_at_position(id.clone(), name, 0.5, 0.5);
        assets.insert(id, path);
    }
}

/// `Overlay::color` is a CSS hex string (the type the TS port carries);
/// `egui::color_edit_button_rgb` wants `[f32; 3]`. These two adapt between
/// them at the widget boundary rather than changing the ported type.
fn hex_to_rgb_f32(hex: &str) -> [f32; 3] {
    opendrop_core::overlay::parse_hex_color(hex).map(|c| c as f32 / 255.0)
}

fn rgb_f32_to_hex(rgb: [f32; 3]) -> String {
    let [r, g, b] = rgb.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
    format!("#{r:02x}{g:02x}{b:02x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::widgets::themed_test_ui;
    use opendrop_core::commands::create_default_registry;
    use opendrop_core::overlay::Overlay;

    fn empty_panel_state() -> (Show, HashMap<String, PathBuf>, u64) {
        (Show::default(), HashMap::new(), 0)
    }

    #[test]
    fn show_does_not_panic_with_no_overlays() {
        themed_test_ui(|ui| {
            let (mut state, mut assets, mut next_id) = empty_panel_state();
            let registry = create_default_registry();
            show(ui, &mut state, &mut assets, &mut next_id, &registry);
        });
    }

    #[test]
    fn show_does_not_panic_dense() {
        themed_test_ui(|ui| {
            widgets::dense(ui, |ui| {
                let (mut state, mut assets, mut next_id) = empty_panel_state();
                let registry = create_default_registry();
                show(ui, &mut state, &mut assets, &mut next_id, &registry);
            });
        });
    }

    #[test]
    fn show_does_not_panic_with_a_text_and_a_media_overlay_expanded() {
        let registry = create_default_registry();
        themed_test_ui(|ui| {
            let (mut state, mut assets, mut next_id) = empty_panel_state();
            let text_id = state.overlay_store.add_text_overlay(mint_id(&mut next_id));
            let media_id = mint_id(&mut next_id);
            state.overlay_store.add_overlay_at_position(media_id.clone(), "photo".to_string(), 0.2, 0.8);
            for id in [text_id, media_id] {
                set_expanded_id(ui, Some(id));
                show(ui, &mut state, &mut assets, &mut next_id, &registry);
            }
        });
    }

    #[test]
    fn show_does_not_panic_in_volume_peak_trigger_mode() {
        themed_test_ui(|ui| {
            let (mut state, mut assets, mut next_id) = empty_panel_state();
            state.overlay_store.queue_enabled = true;
            state.overlay_store.update_overlay_queue_trigger(BeatTriggerConfigPatch {
                mode: Some(BeatTriggerMode::VolumePeak),
                ..Default::default()
            });
            let registry = create_default_registry();
            show(ui, &mut state, &mut assets, &mut next_id, &registry);
        });
    }

    #[test]
    fn show_does_not_panic_with_every_blend_mode() {
        let registry = create_default_registry();
        themed_test_ui(|ui| {
            let (mut state, mut assets, mut next_id) = empty_panel_state();
            for mode in OverlayBlendMode::ALL {
                state.overlay_store.overlays.push(Overlay {
                    id: mint_id(&mut next_id),
                    blend_mode: mode.as_css().to_string(),
                    ..Default::default()
                });
            }
            show(ui, &mut state, &mut assets, &mut next_id, &registry);
        });
    }

    mod mint_id {
        use super::*;

        #[test]
        fn never_repeats_an_id() {
            let mut next_id = 0;
            let ids: Vec<String> = (0..100).map(|_| mint_id(&mut next_id)).collect();
            let unique: std::collections::HashSet<&String> = ids.iter().collect();
            assert_eq!(unique.len(), ids.len());
        }
    }

    mod color_adapters {
        use super::*;

        #[test]
        fn hex_survives_a_round_trip_through_the_widget_representation() {
            for hex in ["#ffffff", "#000000", "#ff2d78", "#00ff80"] {
                assert_eq!(rgb_f32_to_hex(hex_to_rgb_f32(hex)), hex);
            }
        }

        #[test]
        fn an_unparseable_hex_reads_as_white() {
            assert_eq!(hex_to_rgb_f32("not-a-color"), [1.0, 1.0, 1.0]);
        }

        #[test]
        fn out_of_range_channels_are_clamped_not_wrapped() {
            assert_eq!(rgb_f32_to_hex([-1.0, 0.5, 2.0]), "#0080ff");
        }
    }

    mod queue_transport {
        use super::*;

        #[test]
        fn prev_and_next_reach_the_store_through_the_registry() {
            // Recipe B parity, asserted on the dispatch path the ◀/▶
            // buttons use: a real click needs `egui::Context::run` +
            // `Ui::interact`, out of scope for this render-only harness
            // (same limitation `ui::strobe`/`ui::timeline` accept).
            let registry = create_default_registry();
            let mut state = Show::default();
            state.overlay_store.overlays = vec![
                Overlay { id: "a".to_string(), in_queue: true, ..Default::default() },
                Overlay { id: "b".to_string(), in_queue: true, ..Default::default() },
            ];
            registry.dispatch(CommandId::OverlayQueueNext, 1.0, &mut state);
            assert_eq!(state.overlay_store.queue_index, 1);
            registry.dispatch(CommandId::OverlayQueuePrev, 1.0, &mut state);
            assert_eq!(state.overlay_store.queue_index, 0);
        }
    }
}
