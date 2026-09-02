//! Video panel: the background video layer: on/off, opacity, clip
//! rotation, the 4 beat/volume-reactive toggles, a live camera, an NDI
//! source, and the clip library. Port of `SidebarVideo.svelte` (Step 14 of
//! the Phase 8 VJ-panels plan), the last of the 13 sidebars and the only
//! one whose engine half (decode → GL texture → a compositor layer) did not
//! exist in any form beforehand.
//!
//! Same "direct field mutation, no `CommandRegistry::dispatch`" convention
//! as every other panel. No Recipe-B setter is added here on purpose: the
//! plan's transversal command list has no video entry, `CommandId` has no
//! video variant, and adding one would ripple through `io::command_names`,
//! the persisted keymap wire format, and the MIDI mapping file for a
//! control the plan never asked to bind. Noted in this step's report.
//!
//! **Handles never reach this function**, unlike `ui::ndi`/`ui::v4l2loopback`
//! /`ui::osc`: it takes the two *snapshots* it needs to read
//! (`VideoCaptureSnapshot`, `NdiSnapshot`) and returns its one outbound NDI
//! intent through an out-param ([`VideoNdiRequest`], same idiom as
//! `LibraryCtx::load_request`). That is what makes this panel testable at
//! all: every panel in this app that takes a live `*Handle` has no tests,
//! because building one spawns a real I/O thread.
//!
//! Three pieces of state live outside `core::video::VideoState`, because a
//! zero-I/O crate cannot own them, and this panel writes all three:
//! - `clips`: the on-disk clip library (`crate::video_clips`),
//! - `camera_device`: the camera identifier field/dropdown selection,
//! - `local_error`: import/delete failures, which are synchronous on this
//!   thread and so cannot ride in `VideoCaptureSnapshot::last_error` (same
//!   split as `ui::cloud_presets`'s `local_error`).

use opendrop_core::show::Show;
use opendrop_core::video::{VideoAdvance, BEATS_PER_CUT_CHOICES};
use opendrop_io::ndi::{NdiSnapshot, NdiSource};
use opendrop_io::video_capture::{CameraDevice, VideoCaptureSnapshot};

use crate::ui::widgets::{self, theme};
use crate::video_clips::{self, VideoClip, VIDEO_EXTENSIONS};

/// This panel's one outbound intent toward the already-ported NDI-in
/// subsystem: translated into an `NdiControl` message by `main.rs`, which
/// owns the handle. Selecting an NDI source here drives the exact same
/// receiver `ui::ndi::show_in` drives; nothing about NDI is re-ported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VideoNdiRequest {
    Connect(NdiSource),
    Disconnect,
}

#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    show: &mut Show,
    clips: &mut Vec<VideoClip>,
    cameras: &[CameraDevice],
    camera_device: &mut String,
    local_error: &mut Option<String>,
    capture: &VideoCaptureSnapshot,
    ndi: &NdiSnapshot,
    ndi_selected_source: &mut Option<NdiSource>,
    ndi_request: &mut Option<VideoNdiRequest>,
) {
    let ndi_active = ndi.receive_active;
    let external_feed = show.video.external_feed_active(ndi_active);

    ui.horizontal(|ui| {
        ui.heading(format!("Video ({})", clips.len()));
        let label = if show.video.enabled { "⏹ Off" } else { "▶ On" };
        if ui.button(label).clicked() {
            show.video.enabled = !show.video.enabled;
        }
        if ui.button("Rescan").clicked() {
            *clips = video_clips::scan_clips();
            show.video.current_clip_index = 0;
        }
    });

    if let Some(err) = local_error.as_deref() {
        widgets::error_banner(ui, err);
    }
    if let Some(err) = capture.last_error.as_deref() {
        widgets::error_banner(ui, err);
    }

    if show.video.enabled {
        ui.separator();
        opacity_row(ui, &mut show.video.opacity);
        advance_row(ui, show, external_feed);
        reactive_row(ui, show, external_feed);
    }

    ui.separator();
    camera_row(ui, show, cameras, camera_device, local_error, ndi_active, ndi_request);

    ui.separator();
    ndi_row(ui, ndi, ndi_selected_source, ndi_request);

    ui.separator();
    clip_library(ui, show, clips, local_error, external_feed);
}

/// The α crossfader: the layer's own opacity, independent of the deck
/// crossfader (`SidebarVideo.svelte`'s `.crossfader-row`).
fn opacity_row(ui: &mut egui::Ui, opacity: &mut f64) {
    ui.horizontal(|ui| {
        widgets::micro_label(ui, "α");
        ui.add(egui::Slider::new(opacity, 0.0..=1.0).step_by(0.01).show_value(false));
        ui.label(format!("{}%", (*opacity * 100.0).round() as i32));
    });
}

/// Shuffle / Seq / Manual plus the beats-per-cut interval. Disabled while
/// an external feed drives the layer, exactly as the web disabled them
/// (`disabled={liveActive || ndiActive}`): a single camera/NDI stream is
/// not a rotating library.
fn advance_row(ui: &mut egui::Ui, show: &mut Show, external_feed: bool) {
    ui.add_enabled_ui(!external_feed, |ui| {
        ui.horizontal(|ui| {
            let t = theme(ui);
            for mode in VideoAdvance::ALL {
                let color = if show.video.advance == mode { t.palette.accent } else { t.palette.dim };
                if widgets::pill(ui, mode.label(), color).interact(egui::Sense::click()).clicked() {
                    show.video.advance = mode;
                }
            }
            ui.add_enabled_ui(show.video.advance != VideoAdvance::Manual, |ui| {
                egui::ComboBox::from_id_salt("od_video_beats_per_cut")
                    .selected_text(format!("{} beats", show.video.beats_per_cut))
                    .width(90.0)
                    .show_ui(ui, |ui| {
                        for beats in BEATS_PER_CUT_CHOICES {
                            if ui.selectable_label(show.video.beats_per_cut == beats, beats.to_string()).clicked() {
                                show.video.beats_per_cut = beats;
                            }
                        }
                    });
            });
        });
    });
}

/// The 4 beat/volume-reactive toggles, with the same per-toggle disabling
/// the web applied: Cut and Warp need a clip library (and a non-Manual
/// mode, for Cut); Flash and Hue are pure color correction on whatever the
/// layer is showing, so they stay live even for a camera or NDI feed.
fn reactive_row(ui: &mut egui::Ui, show: &mut Show, external_feed: bool) {
    ui.horizontal(|ui| {
        let cut_enabled = !external_feed && show.video.advance != VideoAdvance::Manual;
        toggle_pill(ui, "✂ Cut", &mut show.video.react_cut, cut_enabled, "Cut to another clip on the beat");
        toggle_pill(ui, "✦ Flash", &mut show.video.react_flash, true, "Brighten the layer on the beat");
        toggle_pill(ui, "⏩ Warp", &mut show.video.react_warp, !external_feed, "Speed the clip up with the bass");
        toggle_pill(ui, "🌈 Hue", &mut show.video.react_hue, true, "Rotate the layer's hue on the beat");
    });
}

/// One accent-when-on pill toggle, same widget idiom as `ui::strobe`'s rate
/// buttons. A disabled toggle is drawn dim and does not react to clicks.
fn toggle_pill(ui: &mut egui::Ui, label: &str, value: &mut bool, enabled: bool, hover: &str) {
    let t = theme(ui);
    let color = if *value && enabled { t.palette.accent } else { t.palette.dim };
    let response = widgets::pill(ui, label, color).interact(egui::Sense::click()).on_hover_text(hover);
    if enabled && response.clicked() {
        *value = !*value;
    }
}

/// Live camera: a dropdown of detected devices where the platform can
/// enumerate them (Linux: see `video_capture::list_cameras`), the device
/// field itself, and the on/off toggle.
///
/// The field is shown **always**, not only when the dropdown is empty
/// (review finding): `list_cameras` filters to each physical device's
/// primary capture node, so a device that doesn't follow that convention
/// would otherwise be both absent from the list and unreachable. The
/// dropdown, when there is one, just writes into the same field.
fn camera_row(
    ui: &mut egui::Ui,
    show: &mut Show,
    cameras: &[CameraDevice],
    camera_device: &mut String,
    local_error: &mut Option<String>,
    ndi_active: bool,
    ndi_request: &mut Option<VideoNdiRequest>,
) {
    let camera_on = show.video.live_device.is_some();
    ui.horizontal(|ui| {
        widgets::micro_label(ui, "Camera");
        ui.add_enabled_ui(!camera_on, |ui| {
            if !cameras.is_empty() {
                let selected = cameras
                    .iter()
                    .find(|c| &c.id == camera_device)
                    .map(|c| c.label.as_str())
                    .unwrap_or("(custom device)");
                egui::ComboBox::from_id_salt("od_video_camera").selected_text(selected).width(240.0).show_ui(
                    ui,
                    |ui| {
                        for camera in cameras {
                            if ui.selectable_label(&camera.id == camera_device, &camera.label).clicked() {
                                camera_device.clone_from(&camera.id);
                            }
                        }
                    },
                );
            }
            ui.add(
                egui::TextEdit::singleline(camera_device)
                    .desired_width(180.0)
                    .hint_text("device (e.g. /dev/video0)"),
            );
        });

        let label = if camera_on { format!("📷 {}", show.video.live_label) } else { "📷 Use camera".to_string() };
        if ui.button(label).clicked() {
            if camera_on {
                show.video.clear_live_camera();
            } else {
                // `onToggleLiveCamera`'s own mutual exclusion: only one
                // external feed can drive the layer, and the NDI half of it
                // lives outside `VideoState` (see `core::video`).
                if ndi_active {
                    *ndi_request = Some(VideoNdiRequest::Disconnect);
                }
                match start_camera(show, cameras, camera_device) {
                    Ok(()) => *local_error = None,
                    Err(e) => *local_error = Some(e),
                }
            }
        }
    });
    if !camera_on {
        widgets::micro_label(
            ui,
            if cameras.is_empty() {
                "No camera detected automatically on this platform: type the ffmpeg device \
                 (a DirectShow device name on Windows, an AVFoundation index like 0 on macOS)."
            } else {
                "Only each device's primary capture node is listed. If your camera isn't there, \
                 type its device path."
            },
        );
    }
}

/// Resolves the device field to a `(device, label)` pair and hands it to
/// `VideoState::set_live_camera`. Split out so the "which label goes with
/// this id" rule is testable without a `Ui`.
fn start_camera(show: &mut Show, cameras: &[CameraDevice], camera_device: &str) -> Result<(), String> {
    let device = camera_device.trim();
    if device.is_empty() {
        return Err("Pick a camera (or type its device name) before turning it on.".to_string());
    }
    let label = cameras.iter().find(|c| c.id == device).map(|c| c.label.clone()).unwrap_or_else(|| device.to_string());
    show.video.set_live_camera(device.to_string(), label);
    Ok(())
}

/// NDI as a video source: the same discovered-source list and the same
/// receiver `ui::ndi::show_in` drives, reached from here so the Video panel
/// covers every source kind `SidebarVideo.svelte` offered. Deliberately not
/// a second implementation: `main.rs` turns [`VideoNdiRequest`] straight
/// into the existing `NdiControl` messages.
fn ndi_row(
    ui: &mut egui::Ui,
    ndi: &NdiSnapshot,
    selected_source: &mut Option<NdiSource>,
    ndi_request: &mut Option<VideoNdiRequest>,
) {
    ui.horizontal(|ui| {
        widgets::connection_row(ui, "NDI", ndi.receive_active);
        if ndi.receive_active {
            if ui.button("Disconnect").clicked() {
                *ndi_request = Some(VideoNdiRequest::Disconnect);
            }
        } else if ui.add_enabled(selected_source.is_some(), egui::Button::new("Use NDI")).clicked() {
            if let Some(source) = selected_source.clone() {
                *ndi_request = Some(VideoNdiRequest::Connect(source));
            }
        }
    });

    if ndi.sources.is_empty() {
        widgets::micro_label(ui, "(no NDI sources found)");
        return;
    }
    ui.add_enabled_ui(!ndi.receive_active, |ui| {
        egui::ComboBox::from_id_salt("od_video_ndi_source")
            .selected_text(selected_source.as_ref().map(|s| s.name.as_str()).unwrap_or("select a source"))
            .show_ui(ui, |ui| {
                for source in &ndi.sources {
                    if ui.selectable_label(selected_source.as_ref() == Some(source), &source.name).clicked() {
                        *selected_source = Some(source.clone());
                    }
                }
            });
    });
}

/// Import button, rotation summary, and the clip list itself: a rotation
/// checkbox, the clip name (click to play it now), and a delete button on
/// user clips only.
fn clip_library(
    ui: &mut egui::Ui,
    show: &mut Show,
    clips: &mut Vec<VideoClip>,
    local_error: &mut Option<String>,
    external_feed: bool,
) {
    ui.horizontal(|ui| {
        if ui.button("+ Video").clicked() {
            import_via_file_dialog(show, clips, local_error);
        }
        if !show.video.selected_clip_keys.is_empty() {
            ui.label(format!("{} in rotation", show.video.selected_clip_keys.len()));
            if ui.button("Clear").clicked() {
                show.video.clear_clip_selection();
            }
        }
    });

    if clips.is_empty() {
        widgets::micro_label(ui, "No clips yet: “+ Video” imports one, or drop files in the clip folder (see the Video Loops README).");
        return;
    }
    if external_feed {
        widgets::micro_label(ui, "An external feed is driving the layer: the clip library is paused.");
    }

    let current = show.video.current_clip_index % clips.len();
    let mut to_delete: Option<usize> = None;
    egui::ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
        for (index, clip) in clips.iter().enumerate() {
            ui.push_id(index, |ui| {
                ui.horizontal(|ui| {
                    let mut in_rotation = show.video.selected_clip_keys.iter().any(|k| k == &clip.key);
                    if ui.checkbox(&mut in_rotation, "").on_hover_text("Include in the auto-cut rotation").changed() {
                        show.video.toggle_clip_selection(&clip.key);
                    }
                    let label = if clip.builtin { format!("📦 {}", clip.name) } else { clip.name.clone() };
                    if ui.selectable_label(index == current, label).clicked() {
                        show.video.current_clip_index = index;
                    }
                    if !clip.builtin && ui.small_button("✕").on_hover_text("Delete this clip").clicked() {
                        to_delete = Some(index);
                    }
                });
            });
        }
    });

    if let Some(index) = to_delete {
        delete_clip_at(show, clips, index, local_error);
    }
}

/// Deletes a user clip's file and drops it from the library and the
/// rotation. Port of `removeVideoClip`, whose two bookkeeping steps live in
/// `core::video::VideoState::forget_clip`.
fn delete_clip_at(show: &mut Show, clips: &mut Vec<VideoClip>, index: usize, local_error: &mut Option<String>) {
    let Some(clip) = clips.get(index) else { return };
    if let Err(e) = video_clips::delete_clip(clip) {
        *local_error = Some(e);
        return;
    }
    let key = clips.remove(index).key;
    show.video.forget_clip(&key, clips.len());
    *local_error = None;
}

/// Opens a native "pick video files" dialog (blocking while open, same
/// immediate-mode tradeoff `ui::cloud_presets`'s own Upload button takes)
/// and copies each pick into the user clip folder.
///
/// Mirrors `onVideoFilePick`: multiple files at once, imported in order,
/// and: when 2 or more land: added straight to the auto-cut rotation,
/// since importing a batch *is* the "prepare a playlist" case.
fn import_via_file_dialog(show: &mut Show, clips: &mut Vec<VideoClip>, local_error: &mut Option<String>) {
    let Some(paths) = rfd::FileDialog::new().add_filter("Video", &VIDEO_EXTENSIONS).pick_files() else {
        return; // dialog cancelled
    };
    let mut added: Vec<String> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    for path in paths {
        match video_clips::import_clip(&path) {
            Ok(clip) => {
                added.push(clip.key.clone());
                clips.push(clip);
            }
            Err(e) => errors.push(e),
        }
    }
    if added.len() > 1 {
        for key in &added {
            if !show.video.selected_clip_keys.iter().any(|k| k == key) {
                show.video.toggle_clip_selection(key);
            }
        }
    }
    if !added.is_empty() {
        show.video.enabled = true; // `addVideoFromFile`'s own auto-enable
    }
    *local_error = if errors.is_empty() { None } else { Some(errors.join("; ")) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::widgets::themed_test_ui;
    use std::path::PathBuf;

    fn clip(name: &str, builtin: bool) -> VideoClip {
        VideoClip {
            key: format!("/clips/{name}.webm"),
            name: name.to_string(),
            path: PathBuf::from(format!("/clips/{name}.webm")),
            builtin,
        }
    }

    struct PanelState {
        show: Show,
        clips: Vec<VideoClip>,
        cameras: Vec<CameraDevice>,
        camera_device: String,
        local_error: Option<String>,
        capture: VideoCaptureSnapshot,
        ndi: NdiSnapshot,
        ndi_selected_source: Option<NdiSource>,
        ndi_request: Option<VideoNdiRequest>,
    }

    impl PanelState {
        fn new() -> Self {
            Self {
                show: Show::default(),
                clips: Vec::new(),
                cameras: Vec::new(),
                camera_device: String::new(),
                local_error: None,
                capture: VideoCaptureSnapshot::default(),
                ndi: NdiSnapshot::default(),
                ndi_selected_source: None,
                ndi_request: None,
            }
        }

        fn render(&mut self, ui: &mut egui::Ui) {
            show(
                ui,
                &mut self.show,
                &mut self.clips,
                &self.cameras,
                &mut self.camera_device,
                &mut self.local_error,
                &self.capture,
                &self.ndi,
                &mut self.ndi_selected_source,
                &mut self.ndi_request,
            );
        }
    }

    #[test]
    fn show_does_not_panic_with_an_empty_library() {
        themed_test_ui(|ui| PanelState::new().render(ui));
    }

    #[test]
    fn show_does_not_panic_dense() {
        themed_test_ui(|ui| {
            widgets::dense(ui, |ui| PanelState::new().render(ui));
        });
    }

    #[test]
    fn show_does_not_panic_enabled_in_every_advance_mode() {
        themed_test_ui(|ui| {
            let mut state = PanelState::new();
            state.show.video.enabled = true;
            state.clips = vec![clip("bundled", true), clip("mine", false)];
            for mode in VideoAdvance::ALL {
                state.show.video.advance = mode;
                state.render(ui);
            }
        });
    }

    #[test]
    fn show_does_not_panic_with_a_camera_a_selection_and_errors_on_screen() {
        themed_test_ui(|ui| {
            let mut state = PanelState::new();
            state.show.video.enabled = true;
            state.clips = vec![clip("a", false), clip("b", false)];
            state.show.video.toggle_clip_selection(&state.clips[0].key);
            state.cameras = vec![CameraDevice { id: "/dev/video0".into(), label: "Webcam".into() }];
            state.camera_device = "/dev/video0".into();
            state.show.video.set_live_camera("/dev/video0".into(), "Webcam".into());
            state.local_error = Some("import failed".into());
            state.capture.last_error = Some("ffmpeg exited".into());
            state.render(ui);
        });
    }

    #[test]
    fn show_does_not_panic_while_an_ndi_source_is_receiving() {
        themed_test_ui(|ui| {
            let mut state = PanelState::new();
            state.show.video.enabled = true;
            state.ndi.receive_active = true;
            state.ndi.sources = vec![NdiSource { name: "STUDIO (cam)".into(), address: None }];
            state.ndi_selected_source = state.ndi.sources.first().cloned();
            state.render(ui);
        });
    }

    #[test]
    fn show_does_not_panic_with_an_out_of_range_current_clip_index() {
        // `current_clip_index` is taken modulo the library length
        // everywhere, exactly because a deletion can leave it stale.
        themed_test_ui(|ui| {
            let mut state = PanelState::new();
            state.show.video.enabled = true;
            state.clips = vec![clip("only", false)];
            state.show.video.current_clip_index = 99;
            state.render(ui);
        });
    }

    mod start_camera {
        use super::*;

        #[test]
        fn an_empty_device_field_is_refused_with_a_readable_message() {
            let mut show = Show::default();
            let err = start_camera(&mut show, &[], "   ").unwrap_err();
            assert!(err.contains("Pick a camera"), "unexpected message: {err}");
            assert_eq!(show.video.live_device, None);
        }

        #[test]
        fn a_known_device_gets_its_enumerated_label() {
            let cameras = vec![CameraDevice { id: "/dev/video0".into(), label: "Integrated Camera".into() }];
            let mut show = Show::default();
            start_camera(&mut show, &cameras, "/dev/video0").unwrap();
            assert_eq!(show.video.live_device.as_deref(), Some("/dev/video0"));
            assert_eq!(show.video.live_label, "Integrated Camera");
            assert!(show.video.enabled, "picking a source enables the layer");
        }

        #[test]
        fn a_hand_typed_device_falls_back_to_itself_as_the_label() {
            let mut show = Show::default();
            start_camera(&mut show, &[], "  video=Integrated Webcam  ").unwrap();
            assert_eq!(show.video.live_device.as_deref(), Some("video=Integrated Webcam"));
            assert_eq!(show.video.live_label, "video=Integrated Webcam");
        }
    }

    mod deletion {
        use super::*;

        #[test]
        fn a_failed_delete_leaves_the_library_untouched_and_reports_why() {
            let mut show = Show::default();
            // Bundled clips are never deletable: `delete_clip` refuses.
            let mut clips = vec![clip("bundled", true)];
            let mut error = None;
            delete_clip_at(&mut show, &mut clips, 0, &mut error);
            assert_eq!(clips.len(), 1);
            assert!(error.is_some());
        }

        #[test]
        fn deleting_a_clip_that_no_longer_exists_is_a_no_op() {
            let mut show = Show::default();
            let mut clips: Vec<VideoClip> = Vec::new();
            let mut error = None;
            delete_clip_at(&mut show, &mut clips, 3, &mut error);
            assert!(error.is_none());
        }

        #[test]
        fn deleting_a_real_user_clip_drops_it_from_the_library_and_the_rotation() {
            let dir = std::env::temp_dir().join(format!("opendrop-video-panel-del-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("gone.webm");
            std::fs::write(&path, b"x").unwrap();
            let real = VideoClip {
                key: path.to_string_lossy().into_owned(),
                name: "gone".into(),
                path: path.clone(),
                builtin: false,
            };

            let mut show = Show::default();
            show.video.toggle_clip_selection(&real.key);
            show.video.current_clip_index = 1;
            let mut clips = vec![real];
            let mut error = None;
            delete_clip_at(&mut show, &mut clips, 0, &mut error);

            std::fs::remove_dir_all(&dir).unwrap();
            assert!(error.is_none(), "{error:?}");
            assert!(clips.is_empty());
            assert!(show.video.selected_clip_keys.is_empty());
            assert_eq!(show.video.current_clip_index, 0, "a stale index is clamped");
            assert!(!path.exists());
        }
    }
}
