//! About panel: third-party attribution required by projectM's LGPL-2.1
//! license. This app links libprojectM dynamically
//! (`engine/build.rs`, via pkg-config against the system `libprojectm`
//! package), which keeps the resulting MIT binary free of LGPL
//! contamination: the documented, wiki-endorsed usage for LGPL as a
//! closed/permissively-licensed app's shared-library dependency. The
//! license's condition in return is that this attribution exist: the
//! project's name, a link to it, and (per projectM's own licensing wiki,
//! a generic GitHub link isn't sufficient) a link to the *exact* source
//! release actually linked against.
//!
//! Takes no fields. This panel has no mutable state, unlike every other
//! panel in this app.

pub fn show(ui: &mut egui::Ui) {
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

    ui.separator();

    ui.label("Interface font: Inter.");
    ui.hyperlink_to("Inter", "https://github.com/rsms/inter");
    ui.label("Release used: v4.0");
    ui.hyperlink_to(
        "Exact source release",
        "https://github.com/rsms/inter/releases/download/v4.0/Inter-4.0.zip",
    );
    ui.label("Licensed under the SIL Open Font License 1.1.");

    ui.separator();

    ui.label("Monospace font: JetBrains Mono.");
    ui.hyperlink_to("JetBrains Mono", "https://github.com/JetBrains/JetBrainsMono");
    ui.label("Release used: v2.304");
    ui.hyperlink_to(
        "Exact source release",
        "https://github.com/JetBrains/JetBrainsMono/releases/download/v2.304/JetBrainsMono-2.304.zip",
    );
    ui.label("Licensed under the SIL Open Font License 1.1.");

    ui.separator();

    ui.label("OpenGL ES/EGL provided on Windows by ANGLE, dynamically linked.");
    ui.hyperlink_to("ANGLE", "https://github.com/google/angle");
    ui.label("Licensed under BSD 3-Clause.");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::widgets::themed_test_ui;

    // `show` takes no fields at all (this panel has no mutable state,
    // unlike every other panel in this app), so it's testable directly,
    // no external handle needed.

    #[test]
    fn show_does_not_panic() {
        themed_test_ui(|ui| {
            show(ui);
        });
    }
}
