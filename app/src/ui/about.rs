//! About panel: third-party attribution required by projectM's LGPL-2.1
//! license (PLAN.md Risque 3). This app links libprojectM dynamically
//! (`engine/build.rs`, via pkg-config against the system `libprojectm`
//! package), which keeps the resulting MIT binary free of LGPL
//! contamination: the documented, wiki-endorsed usage for LGPL as a
//! closed/permissively-licensed app's shared-library dependency. The
//! license's condition in return is that this attribution exist: the
//! project's name, a link to it, and (per projectM's own licensing wiki:
//! a generic GitHub link isn't sufficient) a link to the *exact* source
//! release actually linked against.
//!
//! Takes no fields: this panel has no mutable state, unlike every other
//! panel in this app.

pub fn show(ui: &mut egui::Ui) {
    ui.separator();

    ui.label("Visuals rendered by projectM, dynamically linked.");
    ui.hyperlink_to("projectM", "https://github.com/projectM-visualizer/projectm");
    ui.label("Version used: 4.1.6");
    ui.hyperlink_to(
        "Exact source release",
        "https://github.com/projectM-visualizer/projectm/releases/tag/v4.1.6",
    );
    ui.label("Licensed under LGPL-2.1.");

    ui.separator();

    ui.label("Network video powered by NDI®.");
    ui.hyperlink_to("NDI", "https://ndi.video");
    ui.label("NDI® is a registered trademark of Vizrt NDI AB");
}
