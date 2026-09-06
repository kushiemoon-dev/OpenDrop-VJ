//! Output panel: monitor picker for the `output` window + a fullscreen
//! toggle.
//!
//! Monitors are re-queried fresh every frame this panel is visible: no
//! caching, unlike the audio input-device list. This is deliberately
//! different: enumerating monitors is a cheap OS
//! query, re-running it live is what makes hotplug work for free, and there
//! is no bootstrap-time list to keep in sync.
//!
//! Takes individual fields, not `&mut AppState`, same reasoning as the
//! other panels (`ui::decks`, `ui::audio`, `ui::quality`).
//!
//! Audited for the theme pass: no `Color32` literals, the monitor
//! `ComboBox` already re-themes itself
//! automatically (untouched here), and the
//! fullscreen toggle stays a plain `ui.button`, matching the precedent set
//! by other panels' action buttons that were left unstyled (for example
//! playlists' Play/Pause/Prev/Next) rather than converted to a
//! `widgets::primary_button`/`ghost_button` variant. No code change was
//! needed here beyond this note.
//!
//! Not unit-tested: `show` takes `&ActiveEventLoop` and `&Window`, real
//! winit types with no in-crate way to construct an `ActiveEventLoop`
//! outside a running platform event loop (winit provides no test/mock
//! constructor for it), so this panel cannot be exercised under
//! `themed_test_ui`/`__run_test_ui` the way every other panel here is. Same
//! determination `ui::audio` made for `AudioHandle` (a real-hardware-only
//! handle): not fabricating a stand-in for an unmockable external
//! handle.

use winit::event_loop::ActiveEventLoop;
use winit::monitor::MonitorHandle;
use winit::window::{Fullscreen, Window};

pub fn show(
    ui: &mut egui::Ui,
    event_loop: &ActiveEventLoop,
    output_window: &Window,
    selected_output_monitor: &mut Option<String>,
) {
    let monitors: Vec<MonitorHandle> = event_loop.available_monitors().collect();
    let is_fullscreen = output_window.fullscreen().is_some();

    ui.label("Output monitor");
    if monitors.is_empty() {
        ui.label("(no monitors found)");
    } else {
        egui::ComboBox::from_id_salt("output_monitor")
            .selected_text(selected_output_monitor.as_deref().unwrap_or("(current monitor)"))
            .show_ui(ui, |ui| {
                for monitor in &monitors {
                    let name = monitor.name().unwrap_or_else(|| "Unknown".to_string());
                    let is_selected = selected_output_monitor.as_deref() == Some(name.as_str());
                    if ui.selectable_label(is_selected, &name).clicked() {
                        *selected_output_monitor = Some(name.clone());
                        if is_fullscreen {
                            // The picker
                            // used to be inert while the output window was
                            // already fullscreen: the button below reads
                            // "Exit fullscreen" at that point, so there was
                            // no path back through its "enter fullscreen"
                            // branch to pick up a new selection. Retarget
                            // immediately instead, reusing that branch's
                            // own Borderless-on-this-monitor call.
                            let target = monitors.iter().find(|m| m.name().as_deref() == Some(name.as_str())).cloned();
                            output_window.set_fullscreen(Some(Fullscreen::Borderless(target)));
                        }
                    }
                }
            });
    }

    ui.separator();

    if ui.button(if is_fullscreen { "Exit fullscreen" } else { "Fullscreen" }).clicked() {
        if is_fullscreen {
            output_window.set_fullscreen(None);
        } else {
            // Borderless, not Exclusive: no video-mode switch, and standard,
            // hitch-free behavior under Hyprland/Wayland (this project's
            // priority target). `None` here (no selection made yet) falls
            // through to "fullscreen on the current monitor", same as the
            // winit doc comment on `Fullscreen::Borderless` describes.
            let selected_monitor = selected_output_monitor
                .as_deref()
                .and_then(|name| monitors.iter().find(|m| m.name().as_deref() == Some(name)))
                .cloned();
            output_window.set_fullscreen(Some(Fullscreen::Borderless(selected_monitor)));
        }
    }
}
