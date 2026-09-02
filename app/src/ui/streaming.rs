//! Streaming panel: OBS WebSocket connect/disconnect + scene control (Task
//! 16 of the plan), plus Twitch chat and Kick chat connect/disconnect
//! (Task 17). See PHASE5-IO.PLAN's breakdown note for why the three share
//! one file rather than three separate ones (comparable size to the other
//! single-file panels once all three sections exist, not yet the case with
//! only OBS built).
//!
//! Takes individual fields, not `&mut AppState`, same convention as the
//! other panels (`ui::osc`, `ui::remote`). `obs_host`/`obs_port`/
//! `twitch_channel`/`kick_channel` are the panel's own editable fields
//! (`AppState`'s), read by `Connect` at click time: same reasoning as
//! `ui::osc`'s `osc_port`: none of `ObsSnapshot`/`TwitchSnapshot`/
//! `KickSnapshot` has a "what the user is currently typing" concept, only
//! `connected` (+ OBS's `scenes`).
//!
//! Scene switching is one button per scene name (not a dropdown+Go): each
//! click dispatches `ObsControl::SetScene` immediately, and `ObsSnapshot`
//! carries no "current scene" field to highlight a selection against
//! (Override: `CurrentProgramSceneChanged`, OBS->app, isn't ported: see
//! `opendrop_io::obs`'s module doc comment), so there's nothing for a
//! dropdown's "selected" state to reflect anyway.
//!
//! Secret input fields (Twitch OAuth token; Kick bearer/xsrf/cookies) use
//! `egui::TextEdit::password(true)` so the value is masked while typing,
//! write through `io::secrets::set_secret` when the field loses focus
//! (`response.lost_focus()`: in egui this fires both on clicking away AND
//! on pressing Enter in a singleline field, i.e. exactly "blur or submit"
//! in one check), and are cleared back to empty right after: so the typed
//! value is never redisplayed in cleartext once entered, mirroring a
//! normal password-field UX convention. A blur with nothing typed is a
//! no-op (guarded by `is_empty()`) so tabbing through the form can't
//! accidentally overwrite an already-stored secret with a blank one.
//!
//! Reskinned (Step 20 of the Phase 7 UI redesign plan): each of the OBS/
//! Twitch/Kick connected-status rows swaps its own `label(if
//! snapshot.connected {...})` branch for `widgets::connection_row`, same
//! substitution as `ui::midi`/`ui::ndi`/`ui::osc`/`ui::remote` (Steps
//! 17-19). The Connect/Disconnect button pair around each row was
//! otherwise identical across all three services, so that row shape is
//! factored into this file's own `service_connect_row` helper instead of
//! being triplicated verbatim. This is a local helper, not a
//! `connection_row` signature change: unlike `ui::osc`/`ui::remote`'s
//! sibling detail labels, OBS's host+port and Twitch/Kick's channel name
//! don't need a detail label of their own next to the pill, since they're
//! already shown as this panel's own editable fields directly above each
//! row. Every `ui.colored_label(Color32::RED, ...)` on a `last_error`
//! becomes `widgets::error_banner`; Kick's amber "unofficial protocol"
//! disclaimer isn't an error, so it becomes `widgets::warn_banner`
//! instead.

use std::collections::VecDeque;

use opendrop_io::chat::{ChatMessage, ChatPlatform};
use opendrop_io::kick::{KickControl, KickHandle};
use opendrop_io::obs::{ObsControl, ObsHandle};
use opendrop_io::twitch::{TwitchControl, TwitchHandle};

use crate::ui::widgets;

#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    obs: &ObsHandle,
    obs_host: &mut String,
    obs_port: &mut u16,
    twitch: &TwitchHandle,
    twitch_channel: &mut String,
    twitch_oauth_token_input: &mut String,
    kick: &KickHandle,
    kick_channel: &mut String,
    kick_bearer_token_input: &mut String,
    kick_xsrf_token_input: &mut String,
    kick_cookies_input: &mut String,
    chat_log: &VecDeque<ChatMessage>,
    secret_save_error: &mut Option<String>,
) {
    let snapshot = obs.latest();

    ui.label("OBS");

    ui.horizontal(|ui| {
        ui.label("Host");
        ui.add_enabled(!snapshot.connected, egui::TextEdit::singleline(obs_host).desired_width(140.0));
        ui.label("Port");
        ui.add_enabled(!snapshot.connected, egui::DragValue::new(obs_port).range(1..=65535));
    });
    ui.horizontal(|ui| {
        ui.label("Password");
        clear_secret_button(ui, "OBS password", opendrop_io::secrets::OBS_PASSWORD, secret_save_error);
    });

    service_connect_row(
        ui,
        "OBS",
        snapshot.connected,
        || {
            let _ = obs.control_tx.send(ObsControl::Connect(obs_host.clone(), *obs_port));
        },
        || {
            let _ = obs.control_tx.send(ObsControl::Disconnect);
        },
    );
    if let Some(err) = &snapshot.last_error {
        widgets::error_banner(ui, err);
    }

    if snapshot.connected {
        ui.separator();
        ui.label("Scenes");
        if snapshot.scenes.is_empty() {
            ui.label("(no scenes found)");
        } else {
            for scene in &snapshot.scenes {
                if ui.button(scene).clicked() {
                    let _ = obs.control_tx.send(ObsControl::SetScene(scene.clone()));
                }
            }
        }
    }

    ui.separator();
    ui.label("Twitch");

    let twitch_snapshot = twitch.latest();
    ui.horizontal(|ui| {
        ui.label("Channel");
        ui.add_enabled(!twitch_snapshot.connected, egui::TextEdit::singleline(twitch_channel).desired_width(140.0));
    });
    service_connect_row(
        ui,
        "Twitch",
        twitch_snapshot.connected,
        || {
            let _ = twitch.control_tx.send(TwitchControl::Connect(twitch_channel.clone()));
        },
        || {
            let _ = twitch.control_tx.send(TwitchControl::Disconnect);
        },
    );
    if let Some(err) = &twitch_snapshot.last_error {
        widgets::error_banner(ui, err);
    }
    save_secret_field(ui, "OAuth token", twitch_oauth_token_input, opendrop_io::secrets::TWITCH_OAUTH_TOKEN, secret_save_error);

    ui.separator();
    ui.label("Kick");
    widgets::warn_banner(
        ui,
        "unofficial, reverse-engineered protocol: may break without notice if Kick changes its server implementation, no service guarantee",
    );

    let kick_snapshot = kick.latest();
    ui.horizontal(|ui| {
        ui.label("Channel");
        ui.add_enabled(!kick_snapshot.connected, egui::TextEdit::singleline(kick_channel).desired_width(140.0));
    });
    service_connect_row(
        ui,
        "Kick",
        kick_snapshot.connected,
        || {
            let _ = kick.control_tx.send(KickControl::Connect(kick_channel.clone()));
        },
        || {
            let _ = kick.control_tx.send(KickControl::Disconnect);
        },
    );
    if let Some(err) = &kick_snapshot.last_error {
        widgets::error_banner(ui, err);
    }
    save_secret_field(ui, "Bearer token", kick_bearer_token_input, opendrop_io::secrets::KICK_BEARER_TOKEN, secret_save_error);
    save_secret_field(ui, "XSRF token", kick_xsrf_token_input, opendrop_io::secrets::KICK_XSRF_TOKEN, secret_save_error);
    save_secret_field(ui, "Cookies", kick_cookies_input, opendrop_io::secrets::KICK_COOKIES, secret_save_error);

    if let Some(err) = secret_save_error {
        widgets::error_banner(ui, &format!("Secret save failed: {err}"));
    }

    ui.separator();
    ui.label("Chat");
    egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
        // Newest first: a VJ glancing at this panel live wants the latest
        // message visible without having to scroll (whole-branch review
        // Finding 2).
        for msg in chat_log.iter().rev() {
            let platform = match msg.platform {
                ChatPlatform::Twitch => "Twitch",
                ChatPlatform::Kick => "Kick",
            };
            ui.label(format!("[{platform}] {}: {}", msg.username, msg.content));
        }
    });
}

/// A `widgets::connection_row` pill followed by a Connect or Disconnect
/// button, whichever `connected` calls for. Shared by the OBS/Twitch/Kick
/// blocks above, which are otherwise identical in this one row (only the
/// label and the connect/disconnect action differ per service: OBS's
/// `Connect` takes a host+port, Twitch/Kick's a channel name, and each has
/// its own `Handle`/`Control` type). `on_connect`/`on_disconnect` are
/// `FnOnce` so each call site can close over its own handle and fields
/// without this helper needing to know any service-specific type.
fn service_connect_row(ui: &mut egui::Ui, label: &str, connected: bool, on_connect: impl FnOnce(), on_disconnect: impl FnOnce()) {
    ui.horizontal(|ui| {
        widgets::connection_row(ui, label, connected);
        if connected {
            if ui.button("Disconnect").clicked() {
                on_disconnect();
            }
        } else if ui.button("Connect").clicked() {
            on_connect();
        }
    });
}

/// One masked (`password(true)`) text field that writes `input`'s value to
/// the OS keyring under `key` on blur/submit and clears `input` right
/// after: see this module's doc comment for the full UX rationale. Shared
/// by the Twitch OAuth-token field and the 3 Kick credential fields, the
/// only difference between them being the label and the keyring key.
/// `error` is `show`'s shared panel-local save-error field (whole-branch
/// review Finding 1: AC-12): a `set_secret` failure used to be an
/// `eprintln!` only; it's now also written there (and rendered by `show`),
/// and cleared on the next successful save. Also renders a `Clear` button
/// (whole-branch review Finding M8): see `clear_secret_button`.
fn save_secret_field(ui: &mut egui::Ui, label: &str, input: &mut String, key: &str, error: &mut Option<String>) {
    ui.horizontal(|ui| {
        ui.label(label);
        let response = ui.add(egui::TextEdit::singleline(input).password(true).desired_width(220.0));
        if response.lost_focus() && !input.is_empty() {
            match opendrop_io::secrets::set_secret(key, input) {
                Ok(()) => *error = None,
                Err(e) => {
                    eprintln!("opendrop-app: failed to save secret '{key}': {e}");
                    *error = Some(format!("Failed to save secret '{label}': {e}"));
                }
            }
            input.clear();
        }
        clear_secret_button(ui, label, key, error);
    });
}

/// A `Clear` button that deletes the secret stored under `key` from the OS
/// keyring on click: `io::secrets::clear_secret` finally gets a caller
/// (whole-branch review Finding M8: it existed with no way to reach it from
/// the UI, so a saved Twitch/Kick/OBS credential could never be removed
/// again short of editing the OS keyring directly). Never redisplays
/// anything on success: this is a delete action, not a reveal, same
/// "never redisplay in cleartext" discipline as `save_secret_field`. A
/// failure is surfaced through the same panel-local `error` field
/// `save_secret_field` uses for save failures.
fn clear_secret_button(ui: &mut egui::Ui, label: &str, key: &str, error: &mut Option<String>) {
    if ui.button("Clear").clicked() {
        match opendrop_io::secrets::clear_secret(key) {
            Ok(()) => *error = None,
            Err(e) => {
                eprintln!("opendrop-app: failed to clear secret '{key}': {e}");
                *error = Some(format!("Failed to clear secret '{label}': {e}"));
            }
        }
    }
}
