use crate::{thumbnails, AppState, Panel};

/// Per-tick housekeeping for work that runs outside the render path: the preset
/// browser thumbnail pump and the edge-triggered NDI source discovery.
pub(crate) fn service_background_work(state: &mut AppState) {
    // Preset-browser thumbnail pump, at most one unit of
    // work per tick (see `pump_thumbnail_queue`'s own doc comment),
    // and only while the panel that could actually show the result
    // is visible: otherwise this would keep rendering thumbnails no
    // one can see. The `is_some()` half is what reaps a child that
    // was still rendering when the user switched panels; the pump
    // returns as soon as it has handled that child, so it never
    // drains the rest of the queue while the browser is hidden.
    //
    // Unlike the deck loop above, this touches no GL context of its
    // own any more: the render happens in a child process, and all
    // this does here is poll it and upload the bytes it wrote.
    // Deliberately outside the gate below: a render child killed on
    // timeout has to be reaped whether or not the browser panel is
    // still on screen, and this is a `try_wait` per outstanding
    // corpse, normally zero of them.
    thumbnails::reap_killed(&mut state.thumbnail_killed);
    if state.active_panel == Panel::PresetBrowser || state.thumbnail_in_flight.is_some() {
        if let Err(e) = thumbnails::pump_thumbnail_queue(
            &mut state.thumb_queue,
            &mut state.thumbnail_in_flight,
            &mut state.thumbnail_killed,
            &state.thumbnail_cache_dir,
            &state.path_by_name,
            &state.egui_glow.egui_ctx,
            &mut state.thumbnail_textures,
            &mut state.thumbnail_order,
            &mut state.failed_thumbnails,
        ) {
            opendrop_core::log_error!("[app] thumbnail pump failed: {e}");
        }
    }

    // NDI source discovery (see `ndi_discovery_active`'s doc
    // comment): only worth running while the NDI-in panel is on
    // screen to actually show the result. Edge-triggered, not a
    // level check re-sent every tick: `start_discovery` recreates
    // the SDK's `Finder` unconditionally, so resending `Start`
    // every frame while the panel stays open would reopen it 60x/s
    // for nothing.
    let want_ndi_discovery = state.active_panel == Panel::NdiIn;
    if want_ndi_discovery != state.ndi_discovery_active {
        let msg = if want_ndi_discovery {
            opendrop_io::ndi::NdiControl::StartDiscovery
        } else {
            opendrop_io::ndi::NdiControl::StopDiscovery
        };
        let _ = state.ndi.control_tx.send(msg);
        state.ndi_discovery_active = want_ndi_discovery;
    }
}
