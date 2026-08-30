//! Shell chrome (Step 10 of the Phase 7 UI redesign plan): the header
//! (wordmark, hand-painted crossfader, BPM/tap mini-transport, Stage
//! toggle, read-only theme picker), the sectioned nav, and the status bar.
//! `main.rs`'s `ui_root` wires these 3 zones plus the unchanged content
//! `CentralPanel` into the mandated `egui::Panel::top`/`left`/`bottom`/
//! `CentralPanel` order.
//!
//! `header` takes `(&mut ShellCtx, &mut PerformCtx)` as two distinct
//! parameters rather than folding the crossfader/BPM fields into
//! `ShellCtx`: `Show` (the business object both read and write) lives in
//! `PerformCtx` only, never duplicated into another struct (see `ui::ctx`'s
//! module doc comment): same two-struct idiom `ui::preset_browser::show`
//! already established. `status_bar` extends that idiom further: it reads
//! a little from nearly every context struct (connection status, audio,
//! loading activity), so it takes all of them rather than growing a long
//! flat parameter list the way `ui_root` itself used to before Step 9.

use std::time::Instant;

use opendrop_core::show::Show;

use crate::theme::fonts::{FAMILY_MONO, FAMILY_UI_BOLD};
use crate::theme::registry::ThemeId;
use crate::ui::ctx::{ControlCtx, LibraryCtx, OutputCtx, PerformCtx, ShellCtx, SourcesCtx, StreamCtx};
use crate::ui::widgets::{self, theme};
use crate::Panel;

// --- Header --------------------------------------------------------------

/// Always visible regardless of `active_panel` (Step 10 brief): this is
/// the header zone, drawn once per frame before the content match, not
/// gated on which panel is active.
pub fn header(ui: &mut egui::Ui, shell: &mut ShellCtx, perform: &mut PerformCtx) {
    ui.horizontal(|ui| {
        let t = theme(ui);
        wordmark(ui);

        ui.add_space(t.metrics.spacing_airy.x * 2.0);
        crossfader(ui, perform.show);

        ui.add_space(t.metrics.spacing_airy.x * 2.0);
        bpm_tap(ui, perform.show, perform.t0);

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Rightmost: read-only theme picker (Step 10 brief: wiring it
            // to actually switch themes is Step 12's job).
            theme_combo(ui);
            if widgets::ghost_button(ui, "⛶").clicked() {
                *shell.stage_mode = !*shell.stage_mode;
            }
        });
    });
}

/// `OPEN` + `DROP` wordmark: `ui-bold` family alias, accent color on
/// `DROP` only, no gradient text (Step 10 brief).
fn wordmark(ui: &mut egui::Ui) {
    let t = theme(ui);
    let font = egui::FontId::new(t.type_scale.heading, egui::FontFamily::Name(FAMILY_UI_BOLD.into()));
    let mut job = egui::text::LayoutJob::default();
    job.append("OPEN", 0.0, egui::text::TextFormat { font_id: font.clone(), color: t.palette.text, ..Default::default() });
    job.append("DROP", 0.0, egui::text::TextFormat { font_id: font, color: t.palette.accent, ..Default::default() });
    ui.label(job);
}

/// Hand-painted crossfader (Step 10 brief, moved from `ui::decks`: was
/// `decks.rs:47-51`, a plain `egui::Slider`): 1px `dim` ticks, a rail
/// filled with an accent-transparent -> accent@22% gradient (a real
/// GPU-interpolated 2-triangle `Mesh`, not an approximation), a 4px handle
/// with a ~12px glow halo (simulated as several `rect_filled` calls at
/// decreasing opacity/growing radius: the brief's explicitly-allowed
/// technique), and mono `A · NN%` / `NN% · B` labels either side. Still a
/// real drag control, not a static readout: dragging or clicking anywhere
/// along the rail sets `show.crossfader` directly, same end effect as the
/// `Slider` it replaces.
fn crossfader(ui: &mut egui::Ui, show: &mut Show) {
    let t = theme(ui);
    let mono = |size: f32| egui::FontId::new(size, egui::FontFamily::Name(FAMILY_MONO.into()));
    let mono_label = |ui: &mut egui::Ui, text: String| {
        ui.label(egui::RichText::new(text).font(mono(t.type_scale.numeric)).color(t.palette.text));
    };

    mono_label(ui, format!("A · {:.0}%", (1.0 - show.crossfader) * 100.0));

    let rail_width = 200.0;
    let rail_height = 6.0;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(rail_width, 24.0), egui::Sense::click_and_drag());

    if (response.dragged() || response.clicked()) && rect.width() > 0.0 {
        if let Some(pos) = response.interact_pointer_pos() {
            let frac = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            show.crossfader = frac as f64;
        }
    }

    if ui.is_rect_visible(rect) {
        let rail_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(rect.width(), rail_height));

        const TICKS: usize = 11;
        for i in 0..TICKS {
            let x = rail_rect.left() + rail_rect.width() * (i as f32 / (TICKS - 1) as f32);
            ui.painter().line_segment(
                [egui::pos2(x, rail_rect.top() - 3.0), egui::pos2(x, rail_rect.bottom() + 3.0)],
                egui::Stroke::new(1.0, t.palette.dim),
            );
        }

        let mut mesh = egui::Mesh::default();
        let from = t.palette.accent.gamma_multiply(0.0);
        let to = t.palette.accent.gamma_multiply(0.22);
        mesh.colored_vertex(rail_rect.left_top(), from);
        mesh.colored_vertex(rail_rect.left_bottom(), from);
        mesh.colored_vertex(rail_rect.right_top(), to);
        mesh.colored_vertex(rail_rect.right_bottom(), to);
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(2, 1, 3);
        ui.painter().add(mesh);
        ui.painter().rect_stroke(
            rail_rect,
            egui::CornerRadius::from(t.metrics.radius_sm),
            egui::Stroke::new(t.metrics.border_width, t.palette.border),
            egui::StrokeKind::Outside,
        );

        let handle_x = rail_rect.left() + rail_rect.width() * show.crossfader as f32;
        let handle_rect =
            egui::Rect::from_center_size(egui::pos2(handle_x, rail_rect.center().y), egui::vec2(4.0, rail_height + 12.0));
        for (expand, alpha) in [(12.0, 16u8), (8.0, 28u8), (4.0, 46u8)] {
            ui.painter().rect_filled(
                handle_rect.expand(expand),
                egui::CornerRadius::from(t.metrics.radius_lg),
                t.palette.accent.gamma_multiply_u8(alpha),
            );
        }
        ui.painter().rect_filled(handle_rect, egui::CornerRadius::from(t.metrics.radius_sm), t.palette.accent);
    }

    mono_label(ui, format!("{:.0}% · B", show.crossfader * 100.0));
}

/// BPM + Tap Tempo/Clear/beats-per-change/auto-crossfade row, moved
/// verbatim (business logic unchanged) from `ui::playlists`: was
/// `playlists.rs:44-75`, already a single self-contained `ui.horizontal`
/// row there, which is why the whole block (not just the BPM readout)
/// fits the header cleanly. Only the BPM readout itself gets the header's
/// special treatment: mono font with a light glow (Step 10 brief).
fn bpm_tap(ui: &mut egui::Ui, show: &mut Show, t0: Instant) {
    let t = theme(ui);
    ui.horizontal(|ui| {
        let bpm = show.current_bpm();
        let bpm_text = if bpm == 0.0 { "— BPM".to_string() } else { format!("{bpm:.0} BPM") };
        let font = egui::FontId::new(t.type_scale.numeric, egui::FontFamily::Name(FAMILY_MONO.into()));
        let galley = ui.painter().layout_no_wrap(bpm_text, font, t.palette.text);
        let (rect, _) = ui.allocate_exact_size(galley.size(), egui::Sense::hover());
        if ui.is_rect_visible(rect) {
            // Light glow: a single low-opacity accent backdrop, smaller
            // and fainter than the crossfader handle's halo (the brief
            // calls for "un glow léger similaire", not the same
            // intensity).
            ui.painter().rect_filled(
                rect.expand(5.0),
                egui::CornerRadius::from(t.metrics.radius_md),
                t.palette.accent.gamma_multiply(0.08),
            );
            ui.painter().galley(rect.min, galley, t.palette.text);
        }

        if ui.button("Tap Tempo").clicked() {
            show.tap_tempo(t0.elapsed().as_secs_f64() * 1000.0);
        }
        if ui.button("Clear").clicked() {
            show.clear_manual_bpm();
        }
        // Whole-branch review Finding 6 (moved verbatim): `Show::
        // beats_per_change` (auto-crossfade cadence). Same fixed option
        // set as the TS reference's `<select>` (`SidebarPlaylist.svelte:118`).
        egui::ComboBox::from_id_salt("od_beats_per_change")
            .selected_text(show.beats_per_change.to_string())
            .show_ui(ui, |ui| {
                for n in [4u32, 8, 16, 32] {
                    if ui.selectable_label(show.beats_per_change == n, n.to_string()).clicked() {
                        show.beats_per_change = n;
                    }
                }
            });
        // Whole-branch review Finding 7 (moved verbatim): resets the
        // cadence on every toggle (either direction), matching the TS
        // reference's unconditional `resetAutoXfadeCount()` call
        // (`+page.svelte:1754`).
        if ui.toggle_value(&mut show.auto_xfade, "⇄").changed() {
            show.reset_auto_xfade_count();
        }
    });
}

/// Read-only theme picker (Step 10 brief): shows the current theme via a
/// real `ComboBox`, but its entries are plain labels, not
/// selectable/clickable: there is no way to trigger a switch from here
/// yet. Wiring it to actually change the active theme is Step 12's job;
/// the brief is explicit this step should NOT make it interactive, to
/// avoid a "combo visible but does nothing on click" intermediate state.
fn theme_combo(ui: &mut egui::Ui) {
    let t = theme(ui);
    egui::ComboBox::from_id_salt("od_theme_combo").selected_text(format!("{:?}", t.id)).show_ui(ui, |ui| {
        for id in [ThemeId::Kushie, ThemeId::OpenDropClassic, ThemeId::Cyan] {
            ui.label(format!("{id:?}"));
        }
    });
}

// --- Nav -------------------------------------------------------------------

/// Sectioned nav: PERFORM / SOURCES / SORTIE / CONTRÔLE, About pinned at
/// the bottom outside the 4 sections (Step 10 brief).
pub fn nav(ui: &mut egui::Ui, shell: &mut ShellCtx) {
    ui.vertical(|ui| {
        widgets::section(ui, "Perform");
        nav_item(ui, shell.active_panel, Panel::Decks, "Decks");
        // `PresetBrowser` isn't named in this step's brief's PERFORM/
        // SOURCES/SORTIE/CONTRÔLE section listing (13 entries across the 4
        // sections in the default build): a gap in that listing: the
        // brief's own manual-verification line ("les 14 panneaux sont
        // atteignables depuis la nav sectionnée") requires every
        // default-build `Panel` variant reachable, and `PresetBrowser` is
        // the 14th; nothing else in the app can set `active_panel` to it
        // (the old tab row's "Presets" button is gone). Placed here since
        // browsing presets is part of the live-performance workflow.
        // Documented as a judgment call in the task report.
        nav_item(ui, shell.active_panel, Panel::PresetBrowser, "Presets");
        nav_item(ui, shell.active_panel, Panel::Playlists, "Playlists");

        ui.add_space(12.0);
        widgets::section(ui, "Sources");
        nav_item(ui, shell.active_panel, Panel::Audio, "Audio");
        nav_item(ui, shell.active_panel, Panel::Midi, "MIDI");
        nav_item(ui, shell.active_panel, Panel::NdiIn, "NDI In");
        nav_item(ui, shell.active_panel, Panel::Osc, "OSC");
        nav_item(ui, shell.active_panel, Panel::RemoteWs, "Remote");
        nav_item(ui, shell.active_panel, Panel::V4l2, "V4L2");
        #[cfg(feature = "link")]
        nav_item(ui, shell.active_panel, Panel::Link, "Link");

        ui.add_space(12.0);
        widgets::section(ui, "Sortie");
        nav_item(ui, shell.active_panel, Panel::NdiOut, "NDI Out");
        nav_item(ui, shell.active_panel, Panel::Output, "Output");
        nav_item(ui, shell.active_panel, Panel::Streaming, "Streaming");

        ui.add_space(12.0);
        widgets::section(ui, "Contrôle");
        nav_item(ui, shell.active_panel, Panel::Quality, "Quality");

        // About: pinned to the bottom of the nav column, outside the 4
        // sections above (Step 10 brief). A `bottom_up` child layout
        // placed last in the outer `vertical` claims the rest of the
        // panel's height and bottom-anchors its one item inside it:
        // standard egui idiom for a sidebar's pinned-bottom item.
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            nav_item(ui, shell.active_panel, Panel::About, "About");
        });
    });
}

/// One nav row: a `selectable_label` plus, when active, a hand-painted 2px
/// accent rail at its left edge (egui has no border-left primitive: Step
/// 10 brief).
fn nav_item(ui: &mut egui::Ui, active_panel: &mut Panel, panel: Panel, label: &str) {
    let is_active = *active_panel == panel;
    let response = ui.selectable_label(is_active, label);
    if response.clicked() {
        *active_panel = panel;
    }
    if is_active {
        let t = theme(ui);
        let rail = egui::Rect::from_min_size(response.rect.left_top(), egui::vec2(2.0, response.rect.height()));
        ui.painter().rect_filled(rail, egui::CornerRadius::ZERO, t.palette.accent);
    }
}

// --- Status bar --------------------------------------------------------

/// Dense status bar: fps/frame-ms, audio device + VU, one connection
/// pill per service, and preset/thumbnail loading activity (Step 10
/// brief). Pure display: nothing here mutates any context struct's
/// state.
#[allow(clippy::too_many_arguments)]
pub fn status_bar(
    ui: &mut egui::Ui,
    shell: &mut ShellCtx,
    perform: &mut PerformCtx,
    library: &mut LibraryCtx,
    sources: &mut SourcesCtx,
    output: &mut OutputCtx,
    stream: &mut StreamCtx,
    control: &mut ControlCtx,
) {
    // `control` (Ableton Link) is only read from the `#[cfg(feature =
    // "link")]` line below: under the default build `ControlCtx` is its
    // empty marker variant (see that struct's own doc comment) and this
    // line is the only "use" of the parameter, avoiding a spurious
    // unused-variable warning without renaming the param. Same idiom
    // `ui_root` used before `control` was also threaded through here
    // (Step 10).
    let _ = &control;

    widgets::dense(ui, |ui| {
        ui.horizontal(|ui| {
            let (fps_text, frame_text) = match shell.last_wall_ms {
                Some(ms) if ms > 0.0 => (format!("{:.0}FPS", 1000.0 / ms), format!("{ms:.1}MS")),
                _ => ("--FPS".to_string(), "--MS".to_string()),
            };
            widgets::micro_label(ui, &fps_text);
            sep(ui);
            widgets::micro_label(ui, &frame_text);
            sep(ui);

            let device = sources.selected_input_device.as_deref().unwrap_or("(default)");
            widgets::micro_label(ui, device);
            // `vu_meter` sizes itself to `ui.available_width()`, meant for
            // its full-width Audio-panel usage: wrapped in a fixed-size
            // child region here so it doesn't swallow the rest of this
            // dense row (Step 10 judgment call, documented in the task
            // report).
            ui.allocate_ui(egui::vec2(60.0, 12.0), |ui| {
                widgets::vu_meter(ui, sources.last_vu_level as f32);
            });
            sep(ui);

            // Connection pills: one 5px dot per service, in the brief's
            // own listed order: MIDI · NDI · OSC · Remote · OBS · Twitch ·
            // Kick · V4L2 · Link · preset/thumbnail loading activity.
            status_dot(ui, "MIDI", sources.midi.latest().connected);
            let ndi = output.ndi.latest();
            status_dot(ui, "NDI", ndi.composite_active || ndi.receive_active || ndi.deck_active.iter().any(|&a| a));
            status_dot(ui, "OSC", sources.osc.latest().listening);
            status_dot(ui, "REMOTE", sources.remote_ws.latest().listening);
            status_dot(ui, "OBS", stream.obs.latest().connected);
            status_dot(ui, "TWITCH", stream.twitch.latest().connected);
            status_dot(ui, "KICK", stream.kick.latest().connected);
            status_dot(ui, "V4L2", sources.v4l2.latest().running);
            #[cfg(feature = "link")]
            status_dot(ui, "LINK", control.link.latest().enabled);

            // Preset preflight validation / thumbnail queue activity.
            // Approximated from what's visible to `ui::ctx` (Step 10
            // judgment call, documented in the task report: the actual
            // in-flight `--render-thumbnail` child process isn't threaded
            // through any context struct, only `AppState` itself).
            let loading = !perform.pending_validations.is_empty() || !library.thumb_queue.is_empty();
            status_dot(ui, "LOAD", loading);
        });
    });
}

/// A `micro_label`ed 5px dot: `ok` (green) + a soft glow when `active`,
/// `warn` (amber) with no glow otherwise (Step 10 brief).
fn status_dot(ui: &mut egui::Ui, label: &str, active: bool) {
    let t = theme(ui);
    let color = if active { t.palette.ok } else { t.palette.warn };
    let (rect, _) = ui.allocate_exact_size(egui::vec2(9.0, 9.0), egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        let center = rect.center();
        if active {
            ui.painter().circle_filled(center, 6.0, color.gamma_multiply(0.15));
            ui.painter().circle_filled(center, 4.0, color.gamma_multiply(0.32));
        }
        ui.painter().circle_filled(center, 2.5, color);
    }
    widgets::micro_label(ui, label);
}

/// A dim `·` separator between status bar groups.
fn sep(ui: &mut egui::Ui) {
    let t = theme(ui);
    ui.label(egui::RichText::new("·").color(t.palette.dim));
}
