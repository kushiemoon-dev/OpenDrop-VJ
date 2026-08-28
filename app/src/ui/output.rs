//! Output panel: monitor picker for the `output` window + a fullscreen
//! toggle (Step 21 of the plan).
//!
//! Monitors are re-queried fresh every frame this panel is visible: no
//! caching, unlike Step 19's audio input-device list. The brief is explicit
//! that this is deliberately different: enumerating monitors is a cheap OS
//! query, re-running it live is what makes hotplug work for free, and there
//! is no bootstrap-time list to keep in sync.
//!
//! Takes individual fields, not `&mut AppState`, same reasoning as the
//! other panels (`ui::decks`, `ui::audio`, `ui::quality`).

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
                        *selected_output_monitor = Some(name);
                    }
                }
            });
    }

    ui.separator();

    let is_fullscreen = output_window.fullscreen().is_some();
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
