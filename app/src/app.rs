use glutin::prelude::*;
use std::num::NonZeroU32;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::{Key, NamedKey};
use winit::window::WindowId;
use crate::frame::background::service_background_work;
use crate::frame::composite::composite_frame;
use crate::frame::deck_video_sync::sync_deck_video;
use crate::frame::decks::render_decks;
use crate::frame::drain::drain_inputs;
use crate::frame::osc_feedback::publish_osc_state;
use crate::frame::outputs::feed_outputs;
use crate::frame::present::present_frame;
use crate::frame::sources::upload_source_frames;
use crate::frame::tick::{tick_show, TickOutput};
use crate::fatal::{describe_panic, report_fatal};
use crate::frame::ui_pass::run_ui_pass;
use crate::{bootstrap, session, ui_persist, update, App};

fn fail_startup(event_loop: &ActiveEventLoop, detail: &str) {
    report_fatal("OpenDrop could not initialise graphics", detail);
    event_loop.exit();
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        // Bootstrap reports most failures as `Err`, but a few GL/EGL steps
        // can only panic (glutin's config picker cannot return an error), so
        // both are turned into the same message box and a clean exit.
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| bootstrap(event_loop)));
        match outcome {
            Ok(Ok(mut state)) => {
                session::restore(&mut state);
                self.state = Some(state);
            }
            Ok(Err(detail)) => return fail_startup(event_loop, &detail),
            Err(payload) => return fail_startup(event_loop, &describe_panic(payload.as_ref())),
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now()));
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
        let Some(state) = &mut self.state else { return };

        // egui first, control window only; output never carries UI.
        if window_id == state.control.window.id() {
            let _egui_response = state.egui_glow.on_window_event(&state.control.window, &event);
        }

        // Handled regardless of which window has focus; both windows show
        // the same show state, so the keymap isn't per-window. Gated on
        // egui_wants_keyboard_input() (not EventResponse.consumed, which is
        // also true for e.g. a mouse click on a button) so debug shortcuts
        // keep working except while an egui text widget (e.g. the preset
        // browser search) actually has focus.
        if let WindowEvent::KeyboardInput { event: key_event, .. } = &event {
            // `!key_event.repeat` excludes
            // OS auto-repeat: every command dispatched from this path is a
            // discrete press/trigger (deck navigation, toggles), and a
            // held key re-firing one at ~25-30/sec is never the intended
            // UX for any of them, not just the navigation commands that
            // exposed the preflight-flood bug. Filtered globally here
            // rather than per-command.
            if key_event.state == ElementState::Pressed
                && !key_event.repeat
                && !state.egui_glow.egui_ctx.egui_wants_keyboard_input()
            {
                // The Keymap panel's
                // Learn button only records which command is waiting
                // (`keymap_learning`); the key that commits the binding is
                // captured right here, on the very next accepted press,
                // intercepted ahead of normal dispatch (including the F11
                // Stage-mode shortcut below) rather than also firing
                // whatever that key already did/does.
                if let Some(cmd_id) = state.keymap_learning.take() {
                    state.keymap.insert(key_event.logical_key.clone(), cmd_id);
                } else {
                    if let Some(&cmd_id) = state.keymap.get(&key_event.logical_key) {
                        state.registry.dispatch(cmd_id, 1.0, &mut state.show);
                    }

                    // Stage mode is
                    // a transient UI bool on `AppState`, not a `CommandId`:
                    // matched directly against the logical key here rather
                    // than routed through `state.keymap`/`state.registry.
                    // dispatch`, under the same guard as the dispatch above.
                    if key_event.logical_key == Key::Named(NamedKey::F11) {
                        state.stage_mode = !state.stage_mode;
                    }
                }
            }
        }

        let slot = if window_id == state.control.window.id() {
            &mut state.control
        } else if window_id == state.output.window.id() {
            &mut state.output
        } else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Occluded(occluded) => slot.occluded = occluded,
            WindowEvent::Resized(new_size) => {
                slot.size = (new_size.width.max(1), new_size.height.max(1));
                if let (Some(w), Some(h)) = (NonZeroU32::new(slot.size.0), NonZeroU32::new(slot.size.1)) {
                    slot.surface.resize(&state.main_ctx, w, h);
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(state) = &mut self.state else { return };

        drain_inputs(state);

        let now = Instant::now();
        // Wayland can wake this loop for reasons unrelated to pacing (e.g.
        // buffer-release protocol traffic generated by our own previous
        // swap); about_to_wait fires far more often than the WaitUntil
        // deadline requests. Gating the render on next_frame_at, instead of
        // rendering on every call, is what keeps that from turning into a
        // self-sustaining busy loop (measured: ~10 kHz without this gate).
        if now >= state.next_frame_at {
            let TickOutput { mut layer_inputs, audio, now_ms, ndi_active } = tick_show(state, now);

            render_decks(state, now, &audio, &layer_inputs);

            service_background_work(state);

            upload_source_frames(state, ndi_active);

            sync_deck_video(state, now);

            let lowest_active = composite_frame(state, now, now_ms, &mut layer_inputs);

            feed_outputs(state);

            run_ui_pass(state, event_loop);

            present_frame(state, now, lowest_active);

            publish_osc_state(state);

            update::poll(state);

            session::autosave_if_due(state, now);
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(state.next_frame_at));
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(state) = &mut self.state {
            ui_persist::save_ui_config(state, None);
            session::save_now(state);
            if let (Some(path), true) = (crate::preset_scan::scan_file_path(), state.scan.with_table(|t| !t.is_empty())) {
                state.scan.save(&path);
            }

            state.egui_glow.destroy();
        }
    }
}
