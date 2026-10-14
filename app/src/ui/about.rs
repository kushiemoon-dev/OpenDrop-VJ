//! About panel: third-party attribution required by projectM's LGPL-2.1
//! license. This app links libprojectM dynamically
//! (`engine/build.rs`, via pkg-config against the system `libprojectm`
//! package on Linux, the pinned vcpkg overlay port on Windows), which keeps the resulting MIT binary free of LGPL
//! contamination: the documented, wiki-endorsed usage for LGPL as a
//! closed/permissively-licensed app's shared-library dependency. The
//! license's condition in return is that this attribution exist: the
//! project's name, a link to it, and (per projectM's own licensing wiki,
//! a generic GitHub link isn't sufficient) a link to the *exact* source
//! release actually linked against.
//!
//! The version shown is read from the loaded library, so it cannot drift
//! from what is actually linked.
//!
//! Takes no fields. This panel has no mutable state, unlike every other
//! panel in this app.

const PROJECTM_REPO: &str = "https://github.com/projectM-visualizer/projectm";

/// Upstream commit the Windows overlay port builds. Must equal the `REF` in
/// `packaging/windows/overlay-ports/projectm/portfile.cmake`; a test checks it.
const WINDOWS_PROJECTM_COMMIT: &str = "88f23c76743a38c6d8456a8c354c62186270f661";

/// Link to the exact source that was linked: the release tag on Linux, where
/// the system package is a tagged release, and the pinned commit on Windows.
fn projectm_source_url(version: (i32, i32, i32), windows: bool) -> String {
    if windows {
        format!("{PROJECTM_REPO}/tree/{WINDOWS_PROJECTM_COMMIT}")
    } else {
        format!("{PROJECTM_REPO}/releases/tag/v{}.{}.{}", version.0, version.1, version.2)
    }
}

pub fn show(ui: &mut egui::Ui) {
    let version = opendrop_engine::projectm_version();
    ui.label("Visuals rendered by projectM, dynamically linked.");
    ui.hyperlink_to("projectM", PROJECTM_REPO);
    ui.label(format!("Version used: {}.{}.{}", version.0, version.1, version.2));
    ui.hyperlink_to("Exact source release", projectm_source_url(version, cfg!(windows)));
    ui.label("Licensed under LGPL-2.1.");

    ui.separator();

    ui.label("Network video powered by NDI®.");
    ui.hyperlink_to("NDI", "https://ndi.video");
    ui.label("NDI® is a registered trademark of Vizrt NDI AB");

    ui.separator();

    ui.label("Spout output (Windows) uses the Spout SDK, BSD 2-Clause.");
    ui.hyperlink_to("Spout", "https://spout.zeal.co");

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
    fn the_source_link_is_the_tag_on_linux_and_the_pinned_commit_on_windows() {
        assert_eq!(
            projectm_source_url((4, 1, 6), false),
            "https://github.com/projectM-visualizer/projectm/releases/tag/v4.1.6"
        );
        assert_eq!(
            projectm_source_url((4, 2, 0), true),
            format!("https://github.com/projectM-visualizer/projectm/tree/{WINDOWS_PROJECTM_COMMIT}")
        );
    }

    #[test]
    fn the_windows_commit_matches_the_overlay_port() {
        let portfile = include_str!("../../../packaging/windows/overlay-ports/projectm/portfile.cmake");
        assert!(
            portfile.contains(&format!("REF \"{WINDOWS_PROJECTM_COMMIT}\"")),
            "update WINDOWS_PROJECTM_COMMIT when the overlay port's REF changes"
        );
    }

    #[test]
    fn show_does_not_panic() {
        themed_test_ui(|ui| {
            show(ui);
        });
    }
}
