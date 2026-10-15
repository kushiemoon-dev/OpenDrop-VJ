//! About panel: the application version and update notice, a Support tab,
//! and the third-party attribution required by projectM's LGPL-2.1 license.
//! This app links libprojectM dynamically (`engine/build.rs`, via pkg-config
//! against the system `libprojectm` package on Linux, the pinned vcpkg
//! overlay port on Windows), which keeps the resulting MIT binary free of LGPL
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

use opendrop_core::update::ReleaseNotice;

use crate::ui::widgets;

/// Where supporters can contribute. Also the `ko_fi` entry of
/// `.github/FUNDING.yml`, which drives the Sponsor button on GitHub.
const KOFI_URL: &str = "https://ko-fi.com/kushiemoon";

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Tab {
    #[default]
    About,
    Support,
    Log,
}

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

/// The application's own version, from the crate manifest, so it cannot
/// drift from the release.
fn app_version_label() -> String {
    format!("OpenDrop VJ {}", env!("CARGO_PKG_VERSION"))
}

/// About panel: credits and licences, the update notice, a Support tab and
/// the error Log tab.
/// `check_for_updates` is the startup-check switch and `notice` a newer
/// release when there is one; both are informational, nothing is installed.
pub fn show(ui: &mut egui::Ui, check_for_updates: &mut bool, notice: Option<&ReleaseNotice>) {
    let tab_id = egui::Id::new("about_tab");
    let mut tab: Tab = ui.data_mut(|d| d.get_temp(tab_id).unwrap_or_default());
    ui.horizontal(|ui| {
        ui.selectable_value(&mut tab, Tab::About, "About");
        ui.selectable_value(&mut tab, Tab::Support, "Support");
        ui.selectable_value(&mut tab, Tab::Log, "Log");
    });
    ui.data_mut(|d| d.insert_temp(tab_id, tab));
    ui.separator();

    match tab {
        Tab::About => {
            show_updates(ui, check_for_updates, notice);
            ui.separator();
            show_credits(ui);
        }
        Tab::Support => show_support(ui),
        Tab::Log => crate::ui::error_log::show(ui),
    }
}

fn show_updates(ui: &mut egui::Ui, check_for_updates: &mut bool, notice: Option<&ReleaseNotice>) {
    ui.label(format!("You are running {}.", app_version_label()));
    match notice {
        Some(notice) => {
            ui.hyperlink_to(format!("Version {} is available", notice.version), &notice.url);
        }
        None if *check_for_updates => {
            ui.label("No newer release known.");
        }
        None => {
            ui.label("Update checks are off.");
        }
    }
    ui.checkbox(check_for_updates, "Check for a new release when the app starts");
    widgets::micro_label(
        ui,
        "One request to GitHub at most every twelve hours. Nothing is downloaded or installed for you.",
    );
}

fn show_support(ui: &mut egui::Ui) {
    ui.heading("Support OpenDrop VJ");
    ui.label("OpenDrop VJ is free and open source. If it is useful to you, you can support its development.");
    ui.hyperlink_to("Support me on Ko-fi", KOFI_URL);
    widgets::micro_label(ui, "Entirely optional: every feature stays available either way.");
}

fn show_credits(ui: &mut egui::Ui) {
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
    fn the_app_version_comes_from_the_crate_manifest() {
        assert_eq!(app_version_label(), format!("OpenDrop VJ {}", env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn show_does_not_panic_with_and_without_a_notice() {
        themed_test_ui(|ui| {
            let mut check = true;
            show(ui, &mut check, None);
            let notice = ReleaseNotice { version: "9.9.9".to_string(), url: "https://example.com".to_string() };
            show(ui, &mut check, Some(&notice));
            check = false;
            show(ui, &mut check, None);
        });
    }

    #[test]
    fn the_ko_fi_link_matches_the_github_funding_file() {
        let funding = include_str!("../../../.github/FUNDING.yml");
        let account = KOFI_URL.trim_start_matches("https://ko-fi.com/");
        assert!(funding.lines().any(|l| l.trim() == format!("ko_fi: {account}")), "keep KOFI_URL and FUNDING.yml in step");
    }

    #[test]
    fn the_support_tab_renders() {
        themed_test_ui(|ui| {
            show_support(ui);
            show_updates(ui, &mut true, None);
        });
    }
}
