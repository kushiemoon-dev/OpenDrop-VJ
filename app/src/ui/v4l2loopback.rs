//! v4l2loopback output panel: device-found/not-found status plus a
//! Start/Stop button (Task 19 of the plan).
//!
//! `find_device()` is polled exactly once, the first frame this panel is
//! shown, not every frame: mirrors `ui::output`'s doc comment on why its
//! monitor list *is* re-queried every frame ("cheap OS query", hotplug),
//! and explains why this is deliberately unlike that: a v4l2loopback
//! device is even more static than a monitor, it doesn't appear/disappear
//! mid-session the way a monitor can be hotplugged, so there is no
//! hotplug case here worth paying a per-frame `read_dir` for. `device` is
//! `Option<Option<PathBuf>>`: the outer `None` means "not checked yet",
//! populated on first use via `get_or_insert_with`; `Some(None)` means
//! "checked, no device found".
//!
//! Takes individual fields, not `&mut AppState`, same convention as the
//! other panels. `active` is `AppState::v4l2_active` itself (not a
//! separate panel-only toggle re-derived elsewhere); unlike `ui::ndi`,
//! there is no per-slot array to OR together here, just the one stream, so
//! the panel drives the gate flag directly. Resynced from `V4l2Snapshot::
//! running` at the top of every `show` call (whole-branch review Finding
//! M5): if ffmpeg exits on its own (bad/removed device, killed externally),
//! `running` flips to `false` on its own (`io::v4l2loopback::run`'s
//! liveness check), and without this resync the Start/Stop button would
//! stay stuck showing "Stop" for a pipe that no longer exists.
//!
//! Reskinned (Step 19 of the Phase 7 UI redesign plan): the
//! `label(if snapshot.running {...})` running/stopped row swaps for
//! `widgets::connection_row`, same substitution as `ui::midi`/`ui::ndi`
//! (Steps 17-18). The device-found/not-found `match` above it is left
//! untouched: it reports a resolved path, not a binary connected status,
//! so folding it into `connection_row` would drop the path detail for no
//! gain.

use opendrop_io::v4l2loopback::{find_device, V4l2Control, V4l2Handle};
use std::path::PathBuf;

use crate::ui::widgets;

pub fn show(ui: &mut egui::Ui, v4l2: &V4l2Handle, active: &mut bool, device: &mut Option<Option<PathBuf>>) {
    let resolved = device.get_or_insert_with(find_device).clone();

    match &resolved {
        Some(path) => ui.label(format!("Device found: {}", path.display())),
        None => ui.label("Device not found"),
    };

    ui.separator();

    let snapshot = v4l2.latest();
    // See the module doc comment: resync before the button is drawn so a
    // click made this same frame still takes effect and is still sent.
    *active = snapshot.running;
    ui.horizontal(|ui| {
        widgets::connection_row(ui, "v4l2loopback", snapshot.running);
        if *active {
            if ui.button("Stop").clicked() {
                *active = false;
                let _ = v4l2.control_tx.send(V4l2Control::Stop);
            }
        } else if let Some(path) = resolved {
            if ui.button("Start").clicked() {
                *active = true;
                let _ = v4l2.control_tx.send(V4l2Control::Start(path));
            }
        } else {
            ui.add_enabled(false, egui::Button::new("Start"));
        }
    });
}
