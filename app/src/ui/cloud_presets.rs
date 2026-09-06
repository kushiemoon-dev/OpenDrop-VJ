//! CloudPresets panel: upload JSON, copy/link the device token, and a
//! list of the device's cloud presets with rename/delete/download.
//! Port of `SidebarCloudPresets.svelte`.
//!
//! Takes individual fields, not `&mut AppState`, same convention as the
//! other panels (`ui::osc`, `ui::streaming`). `api_url` is this panel's
//! own editable field (`AppState::cloud_presets_api_url`), mirroring
//! `obs_host`; an empty value gates the whole panel down to just that one
//! field (empty means the feature is disabled,
//! same convention as the web app's `PUBLIC_CLOUD_PRESETS_API=`).
//!
//! No "Load onto deck" here, unlike `SidebarCloudPresets.svelte`'s
//! `onLoadPreset`: cloud presets are Butterchurn JSON (the web engine's
//! own format), not this native app's `.milk`/projectM format. This
//! app's loader only reaches projectM through
//! `projectm_load_preset_file`/`projectm_load_preset_data` (both `.milk`
//! text), and no Butterchurn->`.milk` converter exists
//! (confirmed, not merely unverified:
//! building one is explicitly out of scope here; see
//! `opendrop_io::cloud_presets`'s module doc comment). "Download" here
//! only writes the raw JSON to a local cache file and reports the path;
//! it does not touch `Show::preset_catalog`. The format gap is surfaced
//! to the end user directly in this panel (the `warn_banner` above the
//! preset list), not just in this doc comment.
//!
//! No `window.confirm()` equivalent on Delete, unlike
//! `SidebarCloudPresets.svelte`'s `handleDelete`: this codebase has no
//! confirmation-dialog convention anywhere else either (`ui::streaming`'s
//! `clear_secret_button` deletes a stored secret immediately on click),
//! matching that existing convention rather than introducing a new one.
//!
//! Secret input fields (the "link device" token paste) use
//! `egui::TextEdit::password(true)` and are cleared right after
//! submitting, same "never redisplay in cleartext" convention as
//! `ui::streaming`'s `save_secret_field`; this panel's own stored token
//! is never displayed either, only ever copied straight to the clipboard
//! via `Context::copy_text`.

use opendrop_io::cloud_presets::{CloudPresetsControl, CloudPresetsHandle, CLOUD_PRESET_PREFIX};

use crate::ui::widgets;

pub fn show(
    ui: &mut egui::Ui,
    cloud_presets: &CloudPresetsHandle,
    api_url: &mut String,
    token_input: &mut String,
    local_error: &mut Option<String>,
    rename: &mut Option<(String, String)>,
) {
    ui.horizontal(|ui| {
        ui.label("API URL");
        ui.add(egui::TextEdit::singleline(api_url).desired_width(280.0).hint_text("https://presets-cloud.example.workers.dev"));
    });

    if api_url.trim().is_empty() {
        ui.label("Cloud Presets is disabled. Set an API URL above to enable it.");
        return;
    }

    let snapshot = cloud_presets.latest();

    if let Some(err) = local_error.as_deref() {
        widgets::error_banner(ui, err);
    }
    if let Some(err) = snapshot.last_error.as_deref() {
        widgets::error_banner(ui, err);
    }

    ui.separator();

    ui.horizontal(|ui| {
        if ui.button("Copy my token").clicked() {
            match opendrop_io::cloud_presets::ensure_token() {
                Ok(token) => {
                    ui.ctx().copy_text(token);
                    *local_error = None;
                }
                Err(e) => *local_error = Some(format!("Failed to read/create cloud token: {e}")),
            }
        }
        ui.add(egui::TextEdit::singleline(token_input).password(true).desired_width(200.0).hint_text("Link another device (paste token)"));
        let link_clicked = ui.button("Link").clicked();
        if link_clicked && !token_input.trim().is_empty() {
            match opendrop_io::secrets::set_secret(opendrop_io::secrets::CLOUD_PRESETS_TOKEN, token_input.trim()) {
                Ok(()) => {
                    *local_error = None;
                    let _ = cloud_presets.control_tx.send(CloudPresetsControl::List { base_url: api_url.clone() });
                }
                Err(e) => *local_error = Some(format!("Failed to link device: {e}")),
            }
            token_input.clear();
        }
    });

    ui.separator();

    ui.horizontal(|ui| {
        ui.label(format!("My presets ({})", snapshot.entries.len()));
        // `busy` covers every in-flight action (upload/rename/delete too,
        // not just this button's own List), so the label says "Working"
        // rather than something List-specific.
        if ui.button(if snapshot.busy { "Working…" } else { "Refresh" }).clicked() {
            let _ = cloud_presets.control_tx.send(CloudPresetsControl::List { base_url: api_url.clone() });
        }
        if ui.button("+ Upload").clicked() {
            upload_via_file_dialog(cloud_presets, api_url, local_error);
        }
    });

    // Makes the format gap visible in the UI itself, not just in dev-facing
    // doc comments; same `warn_banner` convention `ui::streaming` uses for
    // Kick's "unofficial protocol" disclaimer: a real, non-error caveat the
    // user needs up front, before they click Download expecting a preset
    // to show up on a deck.
    widgets::warn_banner(
        ui,
        "Cloud presets are Butterchurn JSON (the retired web app's format). This app's preset \
         engine (projectM) only loads .milk files, so Download saves the raw JSON to a local \
         cache file for backup/inspection. It can't be loaded onto a deck here.",
    );

    if let Some(path) = snapshot.last_downloaded.as_ref() {
        ui.label(format!("Downloaded (not deck-loadable, see above) to {}", path.display()));
    }

    if snapshot.entries.is_empty() {
        ui.label("No custom presets yet. Upload a JSON file in Butterchurn format.");
        return;
    }

    egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
        for entry in &snapshot.entries {
            ui.horizontal(|ui| {
                let is_renaming = matches!(rename, Some((id, _)) if id == &entry.id);
                if is_renaming {
                    let (_, buffer) = rename.as_mut().expect("is_renaming implies Some");
                    // `lost_focus()` fires on both blur and Enter, same
                    // "blur or submit" convention as `ui::streaming`'s
                    // `save_secret_field`.
                    let response = ui.add(egui::TextEdit::singleline(buffer).desired_width(160.0));
                    if response.lost_focus() || ui.small_button("✓").clicked() {
                        let (id, name) = rename.take().expect("is_renaming implies Some");
                        let _ = cloud_presets.control_tx.send(CloudPresetsControl::Rename { base_url: api_url.clone(), id, name });
                    }
                } else {
                    ui.label(&entry.name);
                    if ui.small_button("✎").clicked() {
                        let stripped = entry.name.strip_prefix(CLOUD_PRESET_PREFIX).unwrap_or(&entry.name);
                        *rename = Some((entry.id.clone(), stripped.to_string()));
                    }
                }
                if ui.small_button("Download").clicked() {
                    let _ = cloud_presets.control_tx.send(CloudPresetsControl::Download { base_url: api_url.clone(), id: entry.id.clone() });
                }
                if ui.small_button("Delete").clicked() {
                    let _ = cloud_presets.control_tx.send(CloudPresetsControl::Delete { base_url: api_url.clone(), id: entry.id.clone() });
                }
            });
        }
    });
}

/// Opens a native "pick a JSON file" dialog (blocks this thread while
/// open, standard immediate-mode-GUI tradeoff, no async file-dialog
/// convention exists anywhere else in this app either) and, if a file was
/// picked, reads it and queues an `Upload`. The upload name is derived
/// from the picked file's name (`.json` extension stripped), mirroring
/// `cloud-presets-store.svelte.ts`'s `onCloudPresetFilePick`
/// (`file.name.replace(/\.json$/i, '')`). Not typed by the user.
fn upload_via_file_dialog(cloud_presets: &CloudPresetsHandle, api_url: &str, local_error: &mut Option<String>) {
    let Some(path) = rfd::FileDialog::new().add_filter("JSON", &["json"]).pick_file() else {
        return; // dialog cancelled
    };
    match std::fs::read_to_string(&path) {
        Ok(data) => {
            let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "preset".to_string());
            *local_error = None;
            let _ = cloud_presets.control_tx.send(CloudPresetsControl::Upload { base_url: api_url.to_string(), name, data });
        }
        Err(e) => *local_error = Some(format!("Failed to read {}: {e}", path.display())),
    }
}
