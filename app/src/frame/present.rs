use std::time::Instant;
use crate::AppState;

/// Renders and swaps both windows, records the frame timing and schedules the next
/// frame deadline.
pub(crate) fn present_frame(state: &mut AppState, now: Instant, lowest_active: Option<usize>) {
    // Two windows, one context: each surface is made current in
    // turn. Skipping render+swap for an Occluded(true) window is
    // load-bearing on Wayland; see the DontWait/WaitUntil comment
    // in bootstrap().
    if let Err(e) = state.control.render_and_swap_with_egui(
        &state.main_ctx,
        &state.gl,
        &state.compositor,
        &mut state.blit_control_timer,
        &mut state.egui_glow,
    ) {
        opendrop_core::log_error!("[app] control window render failed: {e}");
    }
    if let Err(e) =
        state.output.render_and_swap(&state.main_ctx, &state.gl, &state.compositor, &mut state.blit_output_timer)
    {
        opendrop_core::log_error!("[app] output window render failed: {e}");
    }
    // Wall-clock swap-to-swap time is the ground truth for frame
    // time: the GPU pass timers below measure execution time in
    // their own context and never sum into this, since passes
    // across contexts can overlap on real hardware.
    let swap_now = Instant::now();
    let wall_ms = state.last_output_swap_at.map(|prev| (swap_now - prev).as_secs_f64() * 1000.0);
    state.last_output_swap_at = Some(swap_now);
    // Status bar's fps/frame-ms readout, same one-frame
    // staleness as `last_output_swap_at` above, by construction.
    state.last_wall_ms = wall_ms;

    state.perf_tick += 1;
    if state.perf_tick.is_multiple_of(60) {
        // Same expression as `lowest_active`, computed once and reused.
        let active = lowest_active.unwrap_or(0);
        let fmt = |v: Option<f64>| v.map(|ms| format!("{ms:.3}ms")).unwrap_or_else(|| "n/a".to_string());
        println!(
            "[timing] deck{active} render={} copy={} | composite={} | blit control={} output={} | wall(swap-to-swap)={}",
            fmt(state.decks[active].render_ms()),
            fmt(state.decks[active].copy_ms()),
            fmt(state.compositor.composite_ms()),
            fmt(state.blit_control_timer.last_ms()),
            fmt(state.blit_output_timer.last_ms()),
            fmt(wall_ms),
        );
    }

    state.next_frame_at += state.refresh_interval;
    if state.next_frame_at < now {
        state.next_frame_at = now + state.refresh_interval; // fell behind; resync instead of catching up frame-by-frame
    }
}
