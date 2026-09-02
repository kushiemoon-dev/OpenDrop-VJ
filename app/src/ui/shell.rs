//! Shell chrome (Step 10 of the Phase 7 UI redesign plan): the header
//! (wordmark, hand-painted crossfader, BPM/tap mini-transport, Stage
//! toggle, theme picker), the sectioned nav, and the status bar.
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
//!
//! Step 11 adds the Stage-mode counterparts `header_stage` and
//! `status_bar_stage`, wired by `ui_root` via `egui::Panel::show_switched`
//! instead of `header`/`status_bar` while `stage_mode` is on (see that
//! call site's own comment). Both reuse `header`'s hand-painted pieces:
//! `crossfader` directly, and the BPM readout via the `bpm_readout` helper
//! `bpm_tap` was split around: rather than repainting the drag/glow logic
//! a second time.

use std::time::Instant;

use opendrop_core::show::{DeckBus, Show};

use crate::theme::fonts::{FAMILY_MONO, FAMILY_UI_BOLD};
use crate::theme::registry::ThemeId;
use crate::ui::ctx::{ControlCtx, LibraryCtx, OutputCtx, PerformCtx, ShellCtx, SourcesCtx, StreamCtx};
use crate::ui::decks::FLIPPED_V_UV;
use crate::ui::widgets::{self, theme};
use crate::Panel;


// --- Header --------------------------------------------------------------

/// Always visible regardless of `active_panel` (Step 10 brief): this is
/// the header zone, drawn once per frame before the content match, not
/// gated on which panel is active.
pub fn header(ui: &mut egui::Ui, shell: &mut ShellCtx, perform: &mut PerformCtx, theme_request: &mut Option<ThemeId>) {
    // `dense` (Step 8's density-scope helper): the header packs a lot into
    // 48px: wordmark, crossfader, the whole BPM/tap row, and the
    // right-aligned Stage/theme group: so this tightens `item_spacing`
    // between all of them (Step 10 fix-round-1: the airy default measured
    // ~830px of wanted content against a real ~622-800px window at the
    // app's default/untouched size, overflowing the right-aligned group to
    // 0 available width and making it disappear rather than overlap).
    widgets::dense(ui, |ui| {
        ui.horizontal(|ui| {
            let t = theme(ui);
            wordmark(ui);

            ui.add_space(t.metrics.spacing_dense.x);
            crossfader(ui, perform.show);

            ui.add_space(t.metrics.spacing_dense.x);
            bpm_tap(ui, perform.show, perform.t0);

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Rightmost: the theme picker (Step 12: now interactive:
                // see `theme_combo`'s own doc comment).
                theme_combo(ui, theme_request);
                if widgets::ghost_button(ui, "⛶").clicked() {
                    *shell.stage_mode = !*shell.stage_mode;
                }
            });
        });
    });
}

/// Stage-mode header (Step 11): a 28px band replacing `header`'s 48px:
/// wordmark, a `STAGE` mode indicator, no crossfader/BPM/theme controls
/// (those stay in `status_bar_stage`/`header`), but the same `⛶`
/// `ghost_button` `header` uses to toggle `stage_mode`, right-aligned.
/// `F11` is still the primary toggle, but a pointer-driven way out is
/// required too (fix-round-1 review finding): `F11` is a near-universal
/// OS/window-manager fullscreen binding a user's WM could intercept,
/// which would otherwise trap them in Stage mode with no mouse escape.
pub fn header_stage(ui: &mut egui::Ui, stage_mode: &mut bool) {
    let t = theme(ui);
    widgets::dense(ui, |ui| {
        ui.horizontal(|ui| {
            wordmark(ui);
            ui.add_space(t.metrics.spacing_dense.x);
            widgets::pill(ui, "Stage", t.palette.accent);

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::ghost_button(ui, "⛶").clicked() {
                    *stage_mode = !*stage_mode;
                }
            });
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

    // Step 10 fix-round-1: narrowed from 200 to fit the default/untouched
    // window width (see `header`'s own doc comment) without dropping any
    // control: still a full-width drag-anywhere-on-rail target, just more
    // compact.
    let rail_width = 64.0;
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
/// special treatment: mono font with a light glow (Step 10 brief), factored
/// into `bpm_readout` below (Step 11) so `status_bar_stage`'s BPM display
/// can reuse the same hand-painted glow instead of repainting it.
fn bpm_tap(ui: &mut egui::Ui, show: &mut Show, t0: Instant) {
    ui.horizontal(|ui| {
        bpm_readout(ui, show);

        // "Tap" (Step 10 fix-round-1: shortened from "Tap Tempo" to help
        // the header row fit the app's default/untouched window width:
        // see `header`'s own doc comment).
        if ui.button("Tap").clicked() {
            show.tap_tempo(t0.elapsed().as_secs_f64() * 1000.0);
        }
        if ui.button("Clear").clicked() {
            show.clear_manual_bpm();
        }
        // Whole-branch review Finding 6 (moved verbatim): `Show::
        // beats_per_change` (auto-crossfade cadence). Same fixed option
        // set as the TS reference's `<select>` (`SidebarPlaylist.svelte:118`).
        // `.width(40.0)` (Step 10 fix-round-1): egui's `ComboBox` defaults
        // to a 100px minimum width (`Spacing::combo_width`) regardless of
        // content: way oversized for a 1-2 digit value, and a real
        // contributor to the header overflow.
        egui::ComboBox::from_id_salt("od_beats_per_change")
            .width(40.0)
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

/// The BPM readout alone: bare number (or `—` if unknown), mono font, with
/// the light accent glow described on `bpm_tap`. Split out of `bpm_tap`
/// (Step 11 of the Phase 7 UI redesign plan) so `status_bar_stage` can show
/// just the number, without the Tap/Clear/beats-per-change/auto-crossfade
/// controls that stay header-only, while still painting the exact same
/// glow rather than a second hand-rolled copy of it.
fn bpm_readout(ui: &mut egui::Ui, show: &Show) {
    let t = theme(ui);
    let bpm = show.current_bpm();
    // Bare number, no "BPM" unit suffix (Step 10 fix-round-1): sitting
    // directly next to "Tap"/"Clear" already makes it unambiguous, and
    // dropping the 3-letter suffix was part of closing the header
    // overflow found in review: see `header`'s own doc comment.
    let bpm_text = if bpm == 0.0 { "—".to_string() } else { format!("{bpm:.0}") };
    let font = egui::FontId::new(t.type_scale.numeric, egui::FontFamily::Name(FAMILY_MONO.into()));
    let galley = ui.painter().layout_no_wrap(bpm_text, font, t.palette.text);
    let (rect, _) = ui.allocate_exact_size(galley.size(), egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        // Light glow: a single low-opacity accent backdrop, smaller and
        // fainter than the crossfader handle's halo (the brief calls for
        // "a similar light glow", not the same intensity).
        ui.painter().rect_filled(
            rect.expand(5.0),
            egui::CornerRadius::from(t.metrics.radius_md),
            t.palette.accent.gamma_multiply(0.08),
        );
        ui.painter().galley(rect.min, galley, t.palette.text);
    }
}

/// Theme picker (Step 12): shows the current theme via a real `ComboBox`,
/// now interactive: selecting a different entry writes it to the
/// `theme_request` out-param (posed at Step 9) instead of switching
/// anything directly. `ui_root` can't apply a theme change itself (that
/// needs the live `egui::Context`, not available here), so this mirrors
/// `library.load_request`'s idiom: `main.rs` drains `theme_request` once
/// `egui_glow.run()` returns and applies the switch there.
fn theme_combo(ui: &mut egui::Ui, theme_request: &mut Option<ThemeId>) {
    let t = theme(ui);
    // `.width(48.0)` (Step 10 fix-round-1): egui's `ComboBox` defaults to
    // a 100px minimum width regardless of content, a real contributor to
    // the header overflow found in review: this is only a floor, so
    // `OpenDropClassic` (the longest name) still renders in full, just
    // without padding every other, shorter name out to 100px too.
    egui::ComboBox::from_id_salt("od_theme_combo").width(48.0).selected_text(format!("{:?}", t.id)).show_ui(ui, |ui| {
        for id in [ThemeId::Kushie, ThemeId::OpenDropClassic, ThemeId::Cyan] {
            if ui.selectable_label(t.id == id, format!("{id:?}")).clicked() && id != t.id {
                *theme_request = Some(id);
            }
        }
    });
}

// --- Nav -------------------------------------------------------------------

/// Sectioned nav: PERFORM / SOURCES / OUTPUTS / CONTROL, About pinned at
/// the bottom outside the 4 sections (Step 10 brief).
pub fn nav(ui: &mut egui::Ui, shell: &mut ShellCtx) {
    // Filled in by whichever `nav_item` call below is active this frame,
    // used after the `vertical` block to slide the one shared accent rail
    // toward it (Step 22 of the Phase 7 UI redesign plan).
    let mut active_rect: Option<egui::Rect> = None;

    ui.vertical(|ui| {
        // Wrapped in a `ScrollArea` (default settings: shrinks to content
        // height when it fits, scrolls internally otherwise) so the list
        // stays reachable past a fixed-height panel. Found live-testing
        // this phase's nav, which nearly doubled item count from 14 to 27:
        // with no scroll mechanism at all, every item past the panel's
        // fixed height: including "About" below this block: was simply
        // unreachable. Default `auto_shrink` (not forced false) matters:
        // it lets the area shrink below `available_height` when the list
        // fits, which is what leaves room for "About" to still pin at the
        // bottom via its own `bottom_up` block afterward, unchanged.
        egui::ScrollArea::vertical().show(ui, |ui| {
            widgets::section(ui, "Perform");
            nav_item(ui, shell.active_panel, Panel::Decks, "Decks", &mut active_rect);
            // `PresetBrowser` isn't named in this step's brief's PERFORM/
            // SOURCES/OUTPUTS/CONTROL section listing (13 entries across the 4
            // sections in the default build): a gap in that listing: the
            // brief's own manual-verification line ("all 14 panels are
            // reachable from the sectioned nav") requires every
            // default-build `Panel` variant reachable, and `PresetBrowser` is
            // the 14th; nothing else in the app can set `active_panel` to it
            // (the old tab row's "Presets" button is gone). Placed here since
            // browsing presets is part of the live-performance workflow.
            // Documented as a judgment call in the task report.
            nav_item(ui, shell.active_panel, Panel::PresetBrowser, "Presets", &mut active_rect);
            nav_item(ui, shell.active_panel, Panel::Playlists, "Playlists", &mut active_rect);

            ui.add_space(12.0);
            widgets::section(ui, "Sources");
            nav_item(ui, shell.active_panel, Panel::Audio, "Audio", &mut active_rect);
            nav_item(ui, shell.active_panel, Panel::Midi, "MIDI", &mut active_rect);
            nav_item(ui, shell.active_panel, Panel::NdiIn, "NDI In", &mut active_rect);
            nav_item(ui, shell.active_panel, Panel::Osc, "OSC", &mut active_rect);
            nav_item(ui, shell.active_panel, Panel::Overlays, "Overlays", &mut active_rect);
            nav_item(ui, shell.active_panel, Panel::RemoteWs, "Remote", &mut active_rect);
            nav_item(ui, shell.active_panel, Panel::V4l2, "V4L2", &mut active_rect);
            nav_item(ui, shell.active_panel, Panel::Video, "Video", &mut active_rect);
            nav_item(ui, shell.active_panel, Panel::CloudPresets, "Cloud Presets", &mut active_rect);
            #[cfg(feature = "link")]
            nav_item(ui, shell.active_panel, Panel::Link, "Link", &mut active_rect);

            ui.add_space(12.0);
            widgets::section(ui, "Outputs");
            nav_item(ui, shell.active_panel, Panel::NdiOut, "NDI Out", &mut active_rect);
            nav_item(ui, shell.active_panel, Panel::Output, "Output", &mut active_rect);
            nav_item(ui, shell.active_panel, Panel::Streaming, "Streaming", &mut active_rect);
            nav_item(ui, shell.active_panel, Panel::Share, "Share", &mut active_rect);

            ui.add_space(12.0);
            widgets::section(ui, "Control");
            nav_item(ui, shell.active_panel, Panel::Quality, "Quality", &mut active_rect);
            nav_item(ui, shell.active_panel, Panel::Color, "Color", &mut active_rect);
            nav_item(ui, shell.active_panel, Panel::Composite, "Composite", &mut active_rect);
            nav_item(ui, shell.active_panel, Panel::Keymap, "Keymap", &mut active_rect);
            nav_item(ui, shell.active_panel, Panel::Snapshot, "Snapshot", &mut active_rect);
            nav_item(ui, shell.active_panel, Panel::Timeline, "Timeline", &mut active_rect);
            nav_item(ui, shell.active_panel, Panel::Time, "Time", &mut active_rect);
            nav_item(ui, shell.active_panel, Panel::Qvar, "Q-vars", &mut active_rect);
            nav_item(ui, shell.active_panel, Panel::Strobe, "Strobe", &mut active_rect);
            nav_item(ui, shell.active_panel, Panel::Lfo, "LFO", &mut active_rect);
        });

        // About: pinned to the bottom of the nav column, outside the 4
        // sections above (Step 10 brief). A `bottom_up` child layout
        // placed last in the outer `vertical` claims the rest of the
        // panel's height and bottom-anchors its one item inside it:
        // standard egui idiom for a sidebar's pinned-bottom item. Only
        // reachable because the `ScrollArea` above shrinks to its content
        // height instead of always claiming the full available height.
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            nav_item(ui, shell.active_panel, Panel::About, "About", &mut active_rect);
        });
    });

    // Sliding accent rail (Step 22): one persistent rail whose vertical
    // position animates toward the active item's real rect via
    // `animate_value_with_time`, instead of `nav_item` hand-painting a
    // fresh static one at each active item's position every frame (Step
    // 10's original behavior). Fixed `Id`: `nav` has exactly one call site
    // (`main.rs`'s `ui_root`), called once per frame, so a literal key is
    // stable by construction: not a scroll/filter-shifting one.
    if let Some(rect) = active_rect {
        let t = theme(ui);
        let d = t.durations.base.max(4.0 * ui.ctx().input(|i| i.stable_dt));
        let y = ui.ctx().animate_value_with_time(egui::Id::new("od_nav_accent_rail"), rect.top(), d);
        let rail = egui::Rect::from_min_size(egui::pos2(rect.left(), y), egui::vec2(2.0, rect.height()));
        ui.painter().rect_filled(rail, egui::CornerRadius::ZERO, t.palette.accent);
    }
}

/// One nav row: a `selectable_label`. When active, records its rect into
/// `active_rect` (Step 22) so `nav`'s own accent rail: now a single,
/// animated rail slid between active items via `animate_value_with_time`
///: knows where to slide toward, instead of this function hand-painting a
/// fresh static rail at each active item's position every frame (Step 10's
/// original behavior).
fn nav_item(ui: &mut egui::Ui, active_panel: &mut Panel, panel: Panel, label: &str, active_rect: &mut Option<egui::Rect>) {
    let is_active = *active_panel == panel;
    let response = ui.selectable_label(is_active, label);
    if response.clicked() {
        *active_panel = panel;
    }
    if is_active {
        *active_rect = Some(response.rect);
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

/// Stage bottom bar (Step 11): the live-transport counterpart to
/// `status_bar`, shown instead of it while `stage_mode` is on: 4 deck
/// vignettes, a bus A/B deck-count readout, the crossfader, VU, BPM, and
/// FPS, plus the preset drawer's open/close toggle. Deliberately narrower
/// than `status_bar`'s own reads: no connection pills or loading activity
/// here, since those aren't part of the brief's Stage-bar listing and this
/// bar already reuses `crossfader`/`bpm_readout` rather than growing new
/// hand-painted pieces of its own. Takes `SourcesCtx` (for `last_vu_level`)
/// but not `LibraryCtx`/`OutputCtx`/`StreamCtx`/`ControlCtx`: nothing
/// here reads them.
pub fn status_bar_stage(ui: &mut egui::Ui, shell: &mut ShellCtx, perform: &mut PerformCtx, sources: &mut SourcesCtx) {
    let t = theme(ui);
    widgets::dense(ui, |ui| {
        ui.vertical(|ui| {
            // Row 1: what's playing: 4 deck vignettes, the bus A/B
            // deck-count readout, then the crossfader.
            ui.horizontal(|ui| {
                for tex_id in perform.deck_tex_ids {
                    ui.add(egui::Image::new((*tex_id, t.metrics.mini_thumb_size)).uv(FLIPPED_V_UV));
                }
                sep(ui);

                // `accent` = bus A, `ok` = bus B (the same mapping
                // `Metrics`' own doc comment establishes for Step 13's
                // Decks panel), so this reads consistently with the
                // bus-cycle buttons once those are themed.
                let bus_a = perform.show.deck_bus.iter().filter(|&&b| b == DeckBus::A).count();
                let bus_b = perform.show.deck_bus.iter().filter(|&&b| b == DeckBus::B).count();
                widgets::pill(ui, &format!("A {bus_a}"), t.palette.accent);
                widgets::pill(ui, &format!("B {bus_b}"), t.palette.ok);
                sep(ui);

                crossfader(ui, perform.show);
            });

            // Row 2 (live readouts): VU meter, a separator, the BPM
            // readout, Tap/Clear buttons (whole-branch review fix wave,
            // finding 4, added after this row's last real measurement),
            // then a right-aligned group of the FPS label, a separator,
            // and the preset drawer toggle. Split onto its own row rather
            // than packed into row 1 (measured live at the app's default
            // ~624px content width, same instrumented-probe technique
            // Step 10 used for the header's own overflow): `vu_meter`
            // floors its width at `Metrics::tile_content_w` (sized for
            // the Audio panel's own full-width usage) regardless of the
            // space handed to it, which alone left no room for BPM/FPS/
            // the drawer toggle after 4 vignettes + bus pills + the
            // crossfader on one row. Allocated width below reads
            // `t.metrics.tile_content_w` directly (fix-round-1 review
            // finding: a hardcoded `110.0` literal here only avoided
            // clipping because it happened to equal `tile_content_w`'s
            // current value, and would silently re-clip if that token
            // ever changed). Re-measured after Tap/Clear landed
            // (instrumented-probe, same technique): content through the
            // Clear button reaches ~217px, and the right-aligned FPS/
            // separator/toggle group only needs ~60px more, both well
            // under the ~624px available: the row still fits with room
            // to spare, no escalation (tighter spacing, smaller buttons,
            // or a second row) was needed.
            ui.horizontal(|ui| {
                ui.allocate_ui(egui::vec2(t.metrics.tile_content_w, 12.0), |ui| {
                    widgets::vu_meter(ui, sources.last_vu_level as f32);
                });
                sep(ui);

                bpm_readout(ui, perform.show);
                // Whole-branch review fix wave, finding 4: Stage mode (the
                // live-performance mode) had no way to tap or clear the
                // manual BPM at all: `bpm_tap` (the only caller of
                // `Show::tap_tempo`/`clear_manual_bpm`) lives in `header`
                // only, never reached while Stage mode replaces it with
                // `header_stage`. Same controls, same labels as `bpm_tap`,
                // just reachable from here too.
                if ui.button("Tap").clicked() {
                    perform.show.tap_tempo(perform.t0.elapsed().as_secs_f64() * 1000.0);
                }
                if ui.button("Clear").clicked() {
                    perform.show.clear_manual_bpm();
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let fps_text = match shell.last_wall_ms {
                        Some(ms) if ms > 0.0 => format!("{:.0}FPS", 1000.0 / ms),
                        _ => "--FPS".to_string(),
                    };
                    widgets::micro_label(ui, &fps_text);
                    sep(ui);
                    if widgets::ghost_button(ui, "☰").clicked() {
                        *shell.presets_drawer_open = !*shell.presets_drawer_open;
                    }
                });
            });
        });
    });
}

/// A `micro_label`ed 5px dot: `ok` (green) + a soft glow when `active`,
/// `dim` with no glow otherwise (whole-branch review fix wave, finding 5:
/// was `warn`, which made a freshly-launched app's status bar read as a
/// row of warnings for a merely-offline state: `dim` matches `widgets::
/// connection_row`'s established convention for the same conceptual
/// state).
fn status_dot(ui: &mut egui::Ui, label: &str, active: bool) {
    let t = theme(ui);
    let color = if active { t.palette.ok } else { t.palette.dim };
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
