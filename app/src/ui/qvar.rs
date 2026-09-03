//! Qvar panel: the q-var overrides for the currently selected deck slot:
//! a dropdown that adds a watch, then one slider plus a remove button per
//! watched q-var. Backed by `Show::q_var_params` (`DeckQVarParams`, already
//! ported+tested in `core::q_vars`).
//! Port of `SidebarQvar.svelte` (Step 9 of the Phase 8 VJ-panels plan).
//!
//! Takes the whole `[DeckQVarParams; 4]` plus the already-selected index
//! (`Show::selected_slot`) and only renders/mutates that one slot's group;
//! same shape as `ui::composite`/`ui::time`, and for the same reason: the
//! web reference only ever shows the slot named by `mixerSelectedSlot`. The
//! sliders are -2..2 in steps of 0.01, exactly the web app's own range, so
//! there is no unit conversion here. Add and remove go through
//! `q_vars::with_q_var_watch`/`without_q_var_watch` rather than poking
//! `enabled` directly, because those two carry the port's semantics: adding
//! a watch resets its value to 0, removing one leaves the value alone so
//! re-adding is not the same as un-removing. The value sliders mutate their
//! field directly, the same "no `CommandRegistry::dispatch` for the panel's
//! own interactions" convention as the sibling panels: the real setters for
//! keyboard/MIDI/OSC/LFO parity are the 128 `CommandId::Qvar*` commands (see
//! `core::commands`/`core::show`).
//!
//! Unlike Color and Composite, these values are not consumed by this app's
//! own GPU compositor: they are written into each deck's *running projectM
//! preset*. Values ride the one-word-per-deck-per-frame side channel shared
//! with the Time panel (`main.rs`'s `next_param_to_push`); adding or
//! removing a watch instead re-patches and reloads that deck's preset, which
//! restarts its animation. See `engine::qvar_patch` for why that asymmetry
//! exists and `.planning/TIME-QVAR-SPIKE.md` for the channel itself.

use crate::ui::widgets;
use opendrop_core::q_vars::{
    without_q_var_watch, with_q_var_watch, QVarParamsTuple, Q_VAR_COUNT, Q_VAR_MAX, Q_VAR_MIN,
};

/// The web reference's slider step (`SidebarQvar.svelte`).
const STEP: f64 = 0.01;

pub fn show(ui: &mut egui::Ui, q_var_params: &mut QVarParamsTuple, selected_slot: usize) {
    // Applied after the widgets, not inside them: both helpers rebuild the
    // whole `QVarParamsTuple`, which cannot happen while a slider below
    // holds a `&mut` into it.
    let mut add: Option<usize> = None;
    let mut remove: Option<usize> = None;

    {
        let params = &mut q_var_params[selected_slot];
        ui.heading(format!("Q-vars (slot {selected_slot})"));

        // Same filter as the web `<select>`: only q-vars that are not
        // already watched, so the dropdown can never add one twice.
        let available: Vec<usize> = (1..=Q_VAR_COUNT).filter(|&n| !params.enabled[n - 1]).collect();
        // Disabled rather than hidden when all 32 are watched: a control
        // that vanishes reads as a bug, one that greys out reads as "nothing
        // left to add".
        ui.add_enabled_ui(!available.is_empty(), |ui| {
            egui::ComboBox::from_id_salt("od_qvar_add").selected_text("+ Add Q-var").show_ui(ui, |ui| {
                for n in available {
                    // `false`: this is an action list, not a selection:
                    // nothing here is ever the "current" item, same use of
                    // `selectable_label` as `ui::composite`'s blend picker
                    // minus the checked state.
                    if ui.selectable_label(false, format!("Q{n}")).clicked() {
                        add = Some(n);
                    }
                }
            });
        });

        for n in 1..=Q_VAR_COUNT {
            if !params.enabled[n - 1] {
                continue;
            }
            ui.horizontal(|ui| {
                widgets::micro_label(ui, &format!("Q{n}"));
                // egui's slider draws its own numeric readout, so the web
                // row's separate `toFixed(2)` span has no equivalent here.
                ui.add(egui::Slider::new(&mut params.value[n - 1], Q_VAR_MIN..=Q_VAR_MAX).step_by(STEP));
                if ui.button("×").clicked() {
                    remove = Some(n);
                }
            });
        }
    }

    if let Some(n) = add {
        *q_var_params = with_q_var_watch(*q_var_params, selected_slot, n);
    }
    if let Some(n) = remove {
        *q_var_params = without_q_var_watch(*q_var_params, selected_slot, n);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::widgets::themed_test_ui;
    use opendrop_core::q_vars::default_q_var_params;

    fn params() -> QVarParamsTuple {
        [default_q_var_params(); 4]
    }

    #[test]
    fn show_does_not_panic() {
        themed_test_ui(|ui| {
            show(ui, &mut params(), 0);
        });
    }

    #[test]
    fn show_does_not_panic_dense() {
        themed_test_ui(|ui| {
            widgets::dense(ui, |ui| {
                show(ui, &mut params(), 3);
            });
        });
    }

    #[test]
    fn show_renders_every_watch_enabled_at_once_without_panicking() {
        // The row loop and the (now empty, disabled) dropdown at their
        // extreme.
        let mut all = params();
        for n in 0..Q_VAR_COUNT {
            all[1].enabled[n] = true;
            all[1].value[n] = -2.0 + n as f64 * 0.125;
        }
        themed_test_ui(|ui| {
            show(ui, &mut all, 1);
        });
    }

    #[test]
    fn renders_only_the_selected_slot_and_leaves_the_others_alone() {
        let mut p = params();
        p[2].enabled[4] = true;
        p[2].value[4] = 1.5;
        themed_test_ui(|ui| {
            show(ui, &mut p, 2);
        });
        assert_eq!(p[2].value[4], 1.5);
        assert!(p[2].enabled[4]);
        for slot in [0, 1, 3] {
            assert_eq!(p[slot], default_q_var_params(), "slot {slot}");
        }
    }
}
