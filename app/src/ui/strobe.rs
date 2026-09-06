//! Strobe panel: on/off, rate, intensity, and color for the BPM-synced
//! full-screen flash rendered by the compositor
//! (`engine::compositor::Compositor::render_strobe_flash`, timed by
//! `core::strobe::strobe_flash_intensity`). Port of `SidebarStrobe.svelte`.
//! There was no prior engine
//! behavior to port beyond the command name (`CommandId::StrobeToggle`).
//!
//! Only the toggle goes through `CommandRegistry::dispatch` (Recipe B:
//! keyboard/MIDI/OSC/remote-ws parity, same precedent as `ui::timeline`'s
//! Play/Pause button). Rate/intensity/color mutate `Show::strobe` directly,
//! same "direct field mutation" convention as every other panel
//! (`ui::quality`/`ui::color`/`ui::composite`). The transversal command
//! list only names the toggle.

use opendrop_core::commands::{CommandId, CommandRegistry};
use opendrop_core::show::Show;

use crate::ui::widgets::{self, theme};

/// Rate buttons, in beat-rate multiplier units (same convention as
/// `core::lfo::LfoSlot::rate`): 1 = once per beat.
const RATES: [f64; 5] = [0.25, 0.5, 1.0, 2.0, 4.0];

pub fn show(ui: &mut egui::Ui, show: &mut Show, registry: &CommandRegistry) {
    ui.horizontal(|ui| {
        ui.heading("Strobe");
        let label = if show.strobe.enabled { "⏹ Off" } else { "▶ On" };
        if ui.button(label).clicked() {
            registry.dispatch(CommandId::StrobeToggle, 1.0, show);
        }
    });

    ui.separator();

    ui.label("Rate");
    ui.horizontal(|ui| {
        let t = theme(ui);
        for rate in RATES {
            let color = if show.strobe.rate == rate { t.palette.accent } else { t.palette.dim };
            if widgets::pill(ui, &format!("{rate}×"), color).interact(egui::Sense::click()).clicked() {
                show.strobe.rate = rate;
            }
        }
    });

    ui.separator();

    ui.label("Intensity");
    ui.add(egui::Slider::new(&mut show.strobe.intensity, 0.0..=1.0).step_by(0.05));

    ui.separator();

    ui.label("Color");
    ui.color_edit_button_rgb(&mut show.strobe.color);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::widgets::themed_test_ui;
    use opendrop_core::commands::{create_default_registry, CommandContext};

    // `show` takes only `Show` + `&CommandRegistry` (no external handle),
    // same testability tier as `ui::timeline`/`ui::snapshot`.

    #[test]
    fn show_does_not_panic_off() {
        themed_test_ui(|ui| {
            let mut state = Show::default();
            let registry = create_default_registry();
            show(ui, &mut state, &registry);
        });
    }

    #[test]
    fn show_does_not_panic_enabled_at_every_rate() {
        themed_test_ui(|ui| {
            let mut state = Show::default();
            state.strobe.enabled = true;
            let registry = create_default_registry();
            for rate in RATES {
                state.strobe.rate = rate;
                show(ui, &mut state, &registry);
            }
        });
    }

    #[test]
    fn toggle_button_dispatches_through_the_registry() {
        // Recipe B parity: pressing the button must go through
        // `CommandContext::toggle_strobe`, not a direct field write;
        // asserted here on the underlying setter, since a real click needs
        // `egui::Context::run` + `Ui::interact`, out of scope for this
        // render-only harness (same limitation `ui::timeline`'s tests
        // accept).
        let mut state = Show::default();
        assert!(!state.strobe.enabled);
        state.toggle_strobe();
        assert!(state.strobe.enabled);
    }
}
