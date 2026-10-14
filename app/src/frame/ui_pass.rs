use std::sync::Arc;
use winit::event_loop::ActiveEventLoop;
use crate::preset_load::request_preset_load;
use crate::ui_root::ui_root;
use crate::{config, theme, ui, AppState};

/// Runs the egui pass for the control window and applies the requests the panels
/// recorded during it (preset load, video NDI toggle, theme change).
pub(crate) fn run_ui_pass(state: &mut AppState, event_loop: &ActiveEventLoop) {
    // Copied out (Instant/f64 are Copy) before the destructure below
    // so the Playlists panel's Tap Tempo button can compute its own
    // `t0.elapsed()` at click time, and the Audio panel's VU meter
    // can read this tick's level. Neither is named in that
    // destructure, so `state.t0`/`state.last_vu_level` stay readable
    // either way.
    let t0 = state.t0;
    let last_vu_level = state.last_vu_level;
    let energy_level = state.energy_tracker.level();
    // Status bar's fps/frame-ms readout, same by-value
    // copy-before-destructure convention as `last_vu_level` above,
    // since `ShellCtx::last_wall_ms` is a plain `Option<f64>`, not
    // a `&mut` field.
    let last_wall_ms = state.last_wall_ms;
    // Read (and kept alive) before the destructure below, same
    // reason `last_wall_ms` is copied out here: `SourcesCtx` takes
    // a `&VideoCaptureSnapshot`, and the `Arc` it borrows from has
    // to outlive the borrow. A snapshot, not the handle; see
    // `ui::video`'s module doc comment on why no panel handle
    // reaches that panel.
    let video_capture_snapshot = state.video.latest();
    // Per-deck decode-error snapshot, same "copy out before the
    // destructure" reason as
    // `video_capture_snapshot` above; read by the Decks panel to
    // surface a video-mode slot's decode failure on its card.
    let deck_video_errors: [Option<String>; 4] =
        std::array::from_fn(|i| state.deck_video_capture[i].latest().last_error.clone());

    // Destructured so the closure below only
    // borrows the fields it needs, disjoint from `egui_glow` itself
    // (see `ui_root`'s doc comment).
    let AppState {
        egui_glow,
        control,
        output,
        show,
        deck_tex_ids,
        deck_preset_names,
        deck_video_tex_ids,
        pending_validations,
        preset_errors,
        transition_seconds,
        share_set_name,
        active_panel,
        stage_mode,
        presets_drawer_open,
        preset_search_query,
        preset_search_cache,
        favorite_presets,
        favorites_only,
        preset_annotations,
        scan,
        hide_flagged,
        autopilot,
        path_by_name,
        preset_history,
        annotation_edit,
        thumb_queue,
        thumbnail_textures,
        failed_thumbnails,
        audio,
        input_devices,
        selected_input_device,
        audio_gain_db,
        audio_agc,
        beat_sensitivity,
        refresh_interval,
        fps_override,
        ui_scale,
        invisible_mode,
        pending_mesh_size,
        selected_output_monitor,
        registry,
        keymap,
        keymap_learning,
        midi,
        midi_learning,
        ndi,
        ndi_composite_active,
        ndi_deck_active,
        spout,
        spout_composite_active,
        spout_deck_active,
        ndi_in_selected_source,
        spout_in,
        spout_in_selected,
        osc,
        osc_port,
        osc_out,
        osc_out_host,
        osc_out_port,
        osc_out_error,
        rkbx_link,
        rkbx_link_port,
        rkbx_mapping_error,
        overlay_assets,
        next_overlay_id,
        remote_ws,
        obs,
        obs_host,
        obs_port,
        twitch,
        twitch_channel,
        twitch_oauth_token_input,
        kick,
        kick_channel,
        kick_bearer_token_input,
        kick_xsrf_token_input,
        kick_cookies_input,
        chat_log,
        streaming_secret_save_error,
        #[cfg(feature = "link")]
        link,
        #[cfg(feature = "link")]
        link_tempo_input,
        v4l2,
        v4l2_active,
        v4l2_device,
        video_clips,
        video_cameras,
        video_camera_device,
        video_local_error,
        video_panel_target,
        cloud_presets,
        cloud_presets_api_url,
        cloud_presets_token_input,
        cloud_presets_secret_error,
        cloud_presets_rename,
        cloud_video,
        cloud_video_api_url,
        ..
    } = state;
    // Out-param for the preset-browser click path; see `ui_root`'s
    // doc comment. `show` is still borrowed (via the destructure
    // above) for the duration of this closure, so the actual
    // `request_preset_load` call has to wait until after `run()`
    // returns, below.
    let mut preset_load_request: Option<String> = None;
    // Out-param for the Video panel's NDI section, same
    // idiom, same reason: the NDI handle is borrowed by
    // `output_ctx` for the whole closure, so the panel records its
    // intent and the send happens after `run()` returns.
    let mut video_ndi_request: Option<ui::video::VideoNdiRequest> = None;
    // Out-param for a theme switch requested from the UI; see
    // `ui_root`'s own comment on this parameter.
    let mut theme_request: Option<theme::registry::ThemeId> = None;

    // Group the bindings the destructure above just
    // produced into the 7 context structs `ui_root` takes,
    // instead of 51/53 individual arguments. Every field here is
    // exactly one of those bindings. No new state. Named with a
    // `_ctx` suffix only where the destructure already bound a
    // same-named `WindowSlot` (`control`, `output`) that's still
    // needed below for `egui_glow.run`/`&output.window`.
    let mut shell_ctx = ui::ctx::ShellCtx { active_panel, stage_mode, last_wall_ms, presets_drawer_open };
    let mut perform_ctx = ui::ctx::PerformCtx {
        show,
        deck_tex_ids,
        deck_preset_names,
        deck_video_tex_ids,
        deck_video_errors: &deck_video_errors,
        pending_validations,
        preset_errors,
        transition_seconds,
        share_set_name,
        t0,
    };
    let mut library_ctx = ui::ctx::LibraryCtx {
        preset_search_query,
        search_cache: preset_search_cache,
        thumb_queue,
        thumbnail_textures,
        failed_thumbnails,
        load_request: &mut preset_load_request,
        favorite_presets,
        favorites_only,
        preset_annotations,
        scan,
        hide_flagged,
        autopilot,
        energy_level,
        path_by_name,
        preset_history,
        annotation_edit,
    };
    let mut sources_ctx = ui::ctx::SourcesCtx {
        audio,
        input_devices,
        selected_input_device,
        audio_gain_db,
        audio_agc,
        beat_sensitivity,
        last_vu_level,
        registry,
        keymap,
        keymap_learning,
        midi,
        midi_learning,
        ndi_in_selected_source,
        spout_in,
        spout_in_selected,
        osc,
        osc_port,
        osc_out,
        osc_out_host,
        osc_out_port,
        osc_out_error,
        rkbx_link,
        rkbx_link_port,
        rkbx_mapping_error,
        overlay_assets,
        next_overlay_id,
        remote_ws,
        v4l2,
        v4l2_active,
        v4l2_device,
        video_clips,
        video_cameras,
        video_camera_device,
        video_local_error,
        video_capture: &video_capture_snapshot,
        video_ndi_request: &mut video_ndi_request,
        video_panel_target,
        cloud_presets,
        cloud_presets_api_url,
        cloud_presets_token_input,
        cloud_presets_secret_error,
        cloud_presets_rename,
        cloud_video,
        cloud_video_api_url,
    };
    let mut output_ctx = ui::ctx::OutputCtx {
        refresh_interval,
        fps_override,
        ui_scale,
        invisible_mode,
        pending_mesh_size,
        event_loop,
        output_window: &output.window,
        selected_output_monitor,
        ndi,
        ndi_composite_active,
        ndi_deck_active,
        spout,
        spout_composite_active,
        spout_deck_active,
    };
    let mut stream_ctx = ui::ctx::StreamCtx {
        obs,
        obs_host,
        obs_port,
        twitch,
        twitch_channel,
        twitch_oauth_token_input,
        kick,
        kick_channel,
        kick_bearer_token_input,
        kick_xsrf_token_input,
        kick_cookies_input,
        chat_log,
        streaming_secret_save_error,
    };
    let mut control_ctx = ui::ctx::ControlCtx {
        #[cfg(feature = "link")]
        link,
        #[cfg(feature = "link")]
        link_tempo_input,
        #[cfg(not(feature = "link"))]
        _marker: std::marker::PhantomData,
    };
    egui_glow.run(&control.window, |ui| {
        ui_root(
            ui,
            &mut shell_ctx,
            &mut perform_ctx,
            &mut library_ctx,
            &mut sources_ctx,
            &mut output_ctx,
            &mut stream_ctx,
            &mut control_ctx,
            &mut theme_request,
        );
    });
    // The Audio panel's sensitivity slider takes effect immediately.
    show.beat_detector.set_sensitivity(*beat_sensitivity);
    // A click on the Quality panel's scale pills takes effect on the next frame.
    if (egui_glow.egui_ctx.zoom_factor() - *ui_scale).abs() > f32::EPSILON {
        egui_glow.egui_ctx.set_zoom_factor(*ui_scale);
    }
    if let Some(name) = preset_load_request {
        // Same pipeline as keyboard navigation and playlist/beat-sync
        // advances (`take_fired_presets`, above); never a direct
        // deck load, which would bypass the pre-flight validation.
        request_preset_load(state, state.show.selected_slot, name);
    }
    // Video panel's NDI section:
    // translated straight into the already-ported NDI-in
    // subsystem's own control messages: the exact two
    // `ui::ndi::show_in` sends, no second receiver, no second
    // protocol. Selecting NDI also drops any live camera, the
    // other half of `setNdiSource`'s mutual exclusion (the camera
    // half lives in `ui::video::camera_row`).
    match video_ndi_request {
        Some(ui::video::VideoNdiRequest::Connect(source)) => {
            state.show.video.clear_live_camera();
            state.show.video.enabled = true;
            let _ = state.ndi.control_tx.send(opendrop_io::ndi::NdiControl::StartReceive(source));
        }
        Some(ui::video::VideoNdiRequest::Disconnect) => {
            let _ = state.ndi.control_tx.send(opendrop_io::ndi::NdiControl::StopReceive);
        }
        None => {}
    }
    if let Some(new_id) = theme_request {
        // `state.egui_glow`, not the local `egui_glow` binding from
        // the destructure above (its last use is `egui_glow.run(..)`
        // ahead of `preset_load_request`'s handling, which already
        // needs `state` whole again for `request_preset_load`).
        // Same reborrow-after-`state`-is-whole-again idiom that
        // block uses for `state.show`.
        //
        // Same idiom as bootstrap's own theme wiring
        // (`set_style_of` on both `egui::Theme::Dark` and `Light`
        // with the same `Arc<Style>`); `set_fonts` is deliberately
        // not re-called here, only ever at bootstrap.
        let new_theme = theme::registry::get(new_id);
        let new_style = Arc::new(theme::visuals::style(new_theme));
        state.egui_glow.egui_ctx.set_style_of(egui::Theme::Dark, new_style.clone());
        state.egui_glow.egui_ctx.set_style_of(egui::Theme::Light, new_style);
        // Also refresh THEME_ID_KEY in ctx.data, the same key
        // bootstrap wrote: `set_style_of` alone only re-themes native
        // egui controls; `ui::widgets::theme(ui)` reads
        // this key for every custom-painted helper (badges, pills,
        // the crossfader, the nav rail), which would otherwise keep
        // rendering the old theme's palette after this switch.
        state.egui_glow.egui_ctx.data_mut(|d| d.insert_temp(egui::Id::new(theme::THEME_ID_KEY), new_id));

        // Persist immediately (see `config.rs`): read-modify-
        // write just the `theme` field so this doesn't clobber
        // `active_panel`/`stage_mode`/the other already-persisted
        // fields, which are wired at other call sites (bootstrap
        // and `App::exiting`).
        let config_path = config::config_file_path();
        let mut ui_config = config_path.as_deref().map(config::load_config).unwrap_or_default();
        ui_config.theme = new_id;
        config::save_config(config_path.as_deref(), &ui_config);
    }
}
