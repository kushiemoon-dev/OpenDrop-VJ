//! CloudVideo panel: list and download clips from a self-hosted static CDN
//! into the same clip library the Video panel's "+ Video" import button
//! populates. Port of the retired web app's read-only CDN client
//! (`PUBLIC_VIDEO_CDN`), mirroring `ui::cloud_presets`'s `show` function
//! shape.
//!
//! Fetch-only, by design: no upload/rename/delete anywhere in this panel,
//! not even pointed at a user-typed URL (see `opendrop_io::cloud_video`'s
//! module doc comment). `api_url` is this panel's own editable field
//! (`AppState::cloud_video_api_url`), same empty-disables-the-panel
//! convention as `cloud_presets_api_url`; it defaults to empty rather than
//! any working URL, so installing the app never silently reaches out to
//! anyone's personal CDN.
//!
//! A downloaded clip is not added to `clips`/`VideoClip` here: it lands on
//! disk in the shared user clip folder and only shows up in the library on
//! the Video panel's next scan (bootstrap or its own Rescan button), same
//! as a clip dropped into that folder by hand. This panel has no `Vec<
//! VideoClip>` to update even if it wanted to.

use opendrop_io::cloud_video::{CloudVideoControl, CloudVideoHandle, SlugDownloadState};

use crate::ui::widgets;

pub fn show(ui: &mut egui::Ui, cloud_video: &CloudVideoHandle, api_url: &mut String) {
    ui.horizontal(|ui| {
        ui.label("CDN URL");
        ui.add(egui::TextEdit::singleline(api_url).desired_width(280.0).hint_text("https://loops.example.com"));
    });

    if api_url.trim().is_empty() {
        ui.label("Cloud Video is disabled. Set a CDN URL above to enable it.");
        return;
    }

    let snapshot = cloud_video.latest();

    if let Some(err) = snapshot.listing_error.as_deref() {
        widgets::error_banner(ui, err);
    }

    ui.separator();

    ui.horizontal(|ui| {
        ui.label(format!("Cloud clips ({})", snapshot.entries.len()));
        if ui.button(if snapshot.busy { "Working…" } else { "List" }).clicked() {
            let _ = cloud_video.control_tx.send(CloudVideoControl::List { base_url: api_url.clone() });
        }
    });

    if snapshot.entries.is_empty() {
        ui.label("No clips listed yet. Click List to fetch the CDN's catalog.");
        return;
    }

    egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
        for entry in &snapshot.entries {
            ui.horizontal(|ui| {
                ui.label(&entry.name);
                match snapshot.downloads.get(&entry.slug) {
                    Some(SlugDownloadState::InProgress) => {
                        ui.label("Downloading…");
                    }
                    Some(SlugDownloadState::Done) => {
                        ui.label("Downloaded");
                    }
                    Some(SlugDownloadState::AlreadyDownloaded) => {
                        ui.label("Already downloaded");
                    }
                    Some(SlugDownloadState::Failed(err)) => {
                        widgets::error_banner(ui, err);
                        if ui.small_button("Retry").clicked() {
                            let _ = cloud_video
                                .control_tx
                                .send(CloudVideoControl::Download { base_url: api_url.clone(), slug: entry.slug.clone() });
                        }
                    }
                    None => {
                        if ui.small_button("Download").clicked() {
                            let _ = cloud_video
                                .control_tx
                                .send(CloudVideoControl::Download { base_url: api_url.clone(), slug: entry.slug.clone() });
                        }
                    }
                }
            });
        }
    });
}
