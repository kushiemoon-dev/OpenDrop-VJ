use crate::{theme, ui, Panel};

/// Root of the control window's egui content for this frame. `ui` here is
/// already `&mut egui::Ui` (this vendored `egui_glow`'s `EguiGlow::run`
/// hands a `Ui`, not a `Context`; `CentralPanel::show` in this vendored
/// `egui` 0.36.1 matches, taking `ui: &mut Ui` as its first argument).
/// A 4-zone shell: header, sectioned nav, status bar, then content:
/// `egui::Panel::top`/`left`/`bottom` (not `SidePanel`/`TopBottomPanel`,
/// which don't exist in this vendored egui 0.36.1), `CentralPanel` last, in
/// that mandated order. `ui::shell::{header,nav,status_bar}` hold the zone
/// bodies; this function only wires them up plus the unchanged content
/// match inside the `CentralPanel` zone.
///
/// Takes individual `AppState` fields, not `&mut AppState`; see
/// `ui::decks::show`'s doc comment for why. `load_request` is an out-param:
/// the preset-browser panel can't call `request_preset_load` itself (that
/// needs the whole `AppState`), so a click just records the name here for
/// the caller to act on once this frame's egui pass is done.
// The 51 (53 with `--features link`) individual `AppState`-derived values this
// function would otherwise take are grouped into 7 disjoint `&mut` context
// structs (`ui::ctx`) plus `theme_request` as an 8th, standalone out-param.
// See `ui::ctx`'s module doc comment for the field-to-struct assignment and
// the borrow-check reasoning behind it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn ui_root(
    ui: &mut egui::Ui,
    shell: &mut ui::ctx::ShellCtx,
    perform: &mut ui::ctx::PerformCtx,
    library: &mut ui::ctx::LibraryCtx,
    sources: &mut ui::ctx::SourcesCtx,
    output: &mut ui::ctx::OutputCtx,
    stream: &mut ui::ctx::StreamCtx,
    control: &mut ui::ctx::ControlCtx,
    theme_request: &mut Option<theme::registry::ThemeId>,
) {
    // Stage mode retracts the nav
    // and switches the header/status bar to compact, translucent variants
    // while live-mixing. `Panel::show_switched` is a `Panel`-associated
    // function, not a method chained onto one builder as initially assumed;
    // verified against vendored egui 0.36.1 source
    // (`egui-0.36.1/src/containers/panel.rs:563`): it takes the collapsed
    // and expanded `Panel` builders as two separate arguments plus one
    // shared `is_expanded: &mut bool`, and animates the slide between
    // them. Normal is always the "expanded" side, Stage the "collapsed"
    // side, for both the header and the status bar, matching `nav`'s own
    // collapse direction below. Both switches use a *local* `bool` derived
    // fresh from `*shell.stage_mode` every frame (never written back):
    // every panel below is `resizable(false)`, so egui never mutates
    // `is_expanded` internally (drag-to-collapse/expand and the resize
    // double-click both live inside `if resizable { .. }` in
    // `show_inside_dyn`). The only writer of `*shell.stage_mode` is the
    // `F11` handler in `window_event` and the header's own `⛶` button
    // (`ui::shell::header`), and a write-back here would race the latter:
    // the button flips `*shell.stage_mode` *during* this same call, and
    // overwriting it with a value computed before the call would silently
    // discard that click.
    //
    // Distinct `Id`s are mandatory between each zone's Normal/Stage panel
    // (`"od_header"` vs `"od_header_stage"`, `"od_status"` vs `"od_status_
    // stage"`): `show_switched` itself `debug_assert!`s on this. All
    // Stage-mode chrome shares one `Frame::multiply_with_opacity(0.6)`:
    // background + border + shadow modulated together in one operation
    // (`egui-0.36.1/src/containers/frame.rs:313`), never a per-color
    // `gamma_multiply` hack, so the live GL compositor (confirmed opaque
    // and hidden behind Normal-mode chrome) shows through the
    // retracted areas.
    let stage_frame = egui::Frame::side_top_panel(ui.style()).multiply_with_opacity(0.6);

    let mut header_expanded = !*shell.stage_mode;
    egui::Panel::show_switched(
        ui,
        &mut header_expanded,
        egui::Panel::top("od_header_stage").resizable(false).exact_size(28.0).show_separator_line(false).frame(stage_frame),
        egui::Panel::top("od_header").resizable(false).exact_size(48.0).show_separator_line(false),
        |ui, expanded| {
            if expanded {
                ui::shell::header(ui, shell, perform, theme_request);
            } else {
                ui::shell::header_stage(ui, shell.stage_mode);
            }
        },
    );

    let mut nav_open = !*shell.stage_mode;
    egui::Panel::left("od_nav").resizable(false).exact_size(184.0).show_collapsible(ui, &mut nav_open, |ui| {
        ui::shell::nav(ui, shell);
    });

    let mut status_expanded = !*shell.stage_mode;
    egui::Panel::show_switched(
        ui,
        &mut status_expanded,
        egui::Panel::bottom("od_status_stage").resizable(false).exact_size(64.0).frame(stage_frame),
        egui::Panel::bottom("od_status").resizable(false).exact_size(26.0),
        |ui, expanded| {
            if expanded {
                ui::shell::status_bar(ui, shell, perform, library, sources, output, stream, control);
            } else {
                ui::shell::status_bar_stage(ui, shell, perform, sources);
            }
        },
    );

    // Preset drawer: Stage-only, collapsible over the live content
    // (`Panel::bottom("od_presets_drawer").show_collapsible`), toggled by
    // a `ghost_button` inside `status_bar_stage` rather than a new
    // keyboard binding (only `stage_mode`'s toggle needs one). Gated on
    // `*shell.stage_mode`
    // itself, not just left permanently mounted with `presets_drawer_open`
    // pinned to `false`: Normal mode already reaches the same content via
    // `Panel::PresetBrowser` in the nav, so there's no reason to reserve
    // an extra `Panel::bottom` id/slide state for it there.
    if *shell.stage_mode {
        egui::Panel::bottom("od_presets_drawer")
            .resizable(false)
            .exact_size(260.0)
            .frame(stage_frame)
            .show_collapsible(ui, shell.presets_drawer_open, |ui| {
                ui::preset_browser::show(ui, perform, library);
            });
    }

    egui::CentralPanel::default().show(ui, |ui| {
        match shell.active_panel {
            Panel::Decks => {
                ui::decks::show(
                    ui,
                    perform.show,
                    perform.deck_tex_ids,
                    perform.deck_preset_names,
                    sources.video_clips.as_slice(),
                    perform.deck_video_tex_ids,
                    perform.deck_video_errors,
                    perform.pending_validations,
                    perform.preset_errors,
                    perform.transition_seconds,
                );
            }
            Panel::PresetBrowser => {
                ui::preset_browser::show(ui, perform, library);
            }
            Panel::PresetNotes => {
                let current = &perform.deck_preset_names[perform.show.selected_slot];
                ui::preset_notes::show(
                    ui,
                    current,
                    library.preset_annotations,
                    library.preset_history,
                    library.annotation_edit,
                    ui::preset_notes::TrackSettings {
                        memory: library.track_memory,
                        auto_recall: library.track_auto_recall,
                    },
                    library.load_request,
                );
            }
            Panel::Library => {
                ui::library::show(
                    ui,
                    library.scan,
                    library.hide_flagged,
                    library.preset_dir_setting,
                    library.texture_dir_setting,
                    library.path_by_name,
                    library.autopilot,
                    library.energy_level,
                );
            }
            Panel::Playlists => {
                ui::playlists::show(ui, perform.show, library);
            }
            Panel::Audio => {
                ui::audio::show(
                    ui,
                    sources.audio,
                    sources.input_devices,
                    sources.selected_input_device,
                    ui::audio::LevelSettings {
                        gain_db: sources.audio_gain_db,
                        agc: sources.audio_agc,
                        beat_sensitivity: sources.beat_sensitivity,
                    },
                    sources.last_vu_level,
                );
            }
            Panel::Quality => {
                ui::quality::show(
                    ui,
                    output.refresh_interval,
                    output.fps_override,
                    output.ui_scale,
                    output.invisible_mode,
                    output.pending_mesh_size,
                );
            }
            Panel::Color => {
                ui::color::show(ui, &mut perform.show.color_params_a, &mut perform.show.color_params_b);
            }
            Panel::Composite => {
                ui::composite::show(ui, &mut perform.show.slot_composites, perform.show.selected_slot);
            }
            Panel::Keymap => {
                ui::keymap::show(ui, sources.keymap, sources.keymap_learning, sources.registry);
            }
            Panel::Snapshot => {
                ui::snapshot::show(ui, perform.show, sources.registry);
            }
            Panel::Timeline => {
                ui::timeline::show(ui, perform.show, sources.registry);
            }
            Panel::Time => {
                ui::time::show(ui, &mut perform.show.time_params, perform.show.selected_slot);
            }
            Panel::Qvar => {
                ui::qvar::show(ui, &mut perform.show.q_var_params, perform.show.selected_slot);
            }
            Panel::Strobe => {
                ui::strobe::show(ui, perform.show, sources.registry);
            }
            Panel::Lfo => {
                ui::lfo::show(ui, perform.show, sources.registry);
            }
            Panel::Output => {
                ui::output::show(ui, output.event_loop, output.output_window, output.selected_output_monitor);
            }
            Panel::Midi => {
                ui::midi::show(ui, sources.midi, sources.registry, sources.midi_learning);
            }
            Panel::NdiIn => {
                ui::ndi::show_in(ui, output.ndi, sources.ndi_in_selected_source);
            }
            Panel::NdiOut => {
                ui::ndi::show_out(ui, output.ndi, output.ndi_composite_active, output.ndi_deck_active);
            }
            Panel::SpoutIn => {
                ui::spout::show_in(ui, sources.spout_in, sources.spout_in_selected);
            }
            Panel::SpoutOut => {
                ui::spout::show_out(ui, output.spout, output.spout_composite_active, output.spout_deck_active);
            }
            Panel::Osc => {
                ui::osc::show(
                    ui,
                    sources.osc,
                    sources.osc_port,
                    ui::osc::OscOutSettings {
                        sender: sources.osc_out,
                        host: sources.osc_out_host,
                        port: sources.osc_out_port,
                        error: sources.osc_out_error,
                    },
                );
            }
            Panel::RkbxLink => {
                ui::rkbx_link::show(ui, sources.rkbx_link, sources.rkbx_link_port, sources.rkbx_mapping_error, perform.show);
            }
            Panel::Overlays => {
                ui::overlays::show(
                    ui,
                    perform.show,
                    sources.overlay_assets,
                    sources.next_overlay_id,
                    sources.registry,
                );
            }
            Panel::RemoteWs => {
                ui::remote::show(ui, sources.remote_ws);
            }
            Panel::Streaming => {
                ui::streaming::show(
                    ui,
                    stream.obs,
                    stream.obs_host,
                    stream.obs_port,
                    stream.twitch,
                    stream.twitch_channel,
                    stream.twitch_oauth_token_input,
                    stream.kick,
                    stream.kick_channel,
                    stream.kick_bearer_token_input,
                    stream.kick_xsrf_token_input,
                    stream.kick_cookies_input,
                    stream.chat_log,
                    stream.streaming_secret_save_error,
                );
            }
            Panel::Share => {
                ui::share::show(ui, perform.show, perform.deck_preset_names, *perform.transition_seconds, perform.share_set_name);
            }
            #[cfg(feature = "link")]
            Panel::Link => {
                ui::link::show(ui, control.link, control.link_tempo_input);
            }
            Panel::V4l2 => {
                ui::v4l2loopback::show(ui, sources.v4l2, sources.v4l2_active, sources.v4l2_device);
            }
            Panel::Video => {
                // The NDI snapshot is read here, not carried in a context
                // struct: `output.ndi` is a shared borrow of a different
                // struct than every `&mut` this call takes, and the `Arc`
                // only has to outlive this one call.
                let ndi_snapshot = output.ndi.latest();
                let spout_senders = sources.spout_in.latest().senders.clone();
                ui::video::show(
                    ui,
                    perform.show,
                    sources.video_clips,
                    sources.video_cameras,
                    sources.video_camera_device,
                    sources.video_local_error,
                    sources.video_capture,
                    &ndi_snapshot,
                    sources.ndi_in_selected_source,
                    sources.video_ndi_request,
                    sources.video_panel_target,
                    &spout_senders,
                    sources.spout_in_selected,
                    ui::video::TrackLinks { current_track: library.track_memory.current(), links: sources.video_links },
                );
            }
            Panel::CloudPresets => {
                ui::cloud_presets::show(
                    ui,
                    sources.cloud_presets,
                    sources.cloud_presets_api_url,
                    sources.cloud_presets_token_input,
                    sources.cloud_presets_secret_error,
                    sources.cloud_presets_rename,
                );
            }
            Panel::CloudVideo => {
                ui::cloud_video::show(ui, sources.cloud_video, sources.cloud_video_api_url);
            }
            Panel::About => {
                ui::about::show(ui, shell.check_for_updates, shell.update_notice.as_ref());
            }
        }
    });
}
