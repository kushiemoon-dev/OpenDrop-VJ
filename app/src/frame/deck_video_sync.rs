use opendrop_engine::deck::{self};
use std::time::Instant;
use crate::deck_video::{desired_video_input, rkbx_drift_exceeds_threshold, tick_deck_video, RkbxSyncState};
use crate::preset_load::request_preset_load;
use crate::{video_clips, AppState};
use opendrop_core::track_memory::TrackMemory;

/// Deck video sources: applies rekordbox track changes to per-deck clips, then
/// reconciles and uploads each deck's decoded video frame.
pub(crate) fn sync_deck_video(state: &mut AppState, now: Instant) {
    // rkbx_link track-change -> match -> load: drains
    // discrete track-changed events into a direct per-slot clip
    // load, never touching that slot's bus assignment. No
    // match found leaves the mapped slot's current content
    // untouched.
    while let Ok(event) = state.rkbx_link.track_events.try_recv() {
        on_track_changed(state, &event.artist, &event.title);
        let Some(slot) = state.show.rkbx_deck_mapping.get(event.deck).copied().flatten() else { continue };
        let Some(clip) = video_clips::match_clip_by_track(&state.video_clips, &event.artist, &event.title) else { continue };
        let path = clip.path.clone();
        let index = state.video_clips.iter().position(|c| c.path == path).unwrap_or(0);
        state.show.deck_video[slot].current_clip_index = index;
        state.show.deck_video[slot].enabled = true;
        let seek_seconds = state.rkbx_link.latest().deck_time[event.deck].unwrap_or(0.0);
        state.rkbx_sync[slot] = Some(RkbxSyncState { dj_deck: event.deck, matched_clip_path: path, seek_seconds, seeked_at: now });
    }

    // rkbx_link drift correction: reseeks a synced
    // slot whenever its estimated elapsed playback diverges from
    // its DJ deck's latest reported time by more than
    // `RKBX_DRIFT_THRESHOLD_SECONDS`. Drops the sync (without
    // touching the slot's content) if the mapping was changed or
    // the VJ manually reassigned the slot away from the matched
    // clip since the last match/reseek: manual action supersedes
    // sync.
    for slot in 0..deck::DECK_COUNT {
        let Some(sync) = &state.rkbx_sync[slot] else { continue };
        let mapping_still_points_here = state.show.rkbx_deck_mapping.get(sync.dj_deck).copied().flatten() == Some(slot);
        let current_clip_path = state
            .video_clips
            .get(state.show.deck_video[slot].current_clip_index % state.video_clips.len().max(1))
            .map(|c| c.path.clone());
        let still_the_matched_clip = state.show.deck_video[slot].enabled && current_clip_path.as_ref() == Some(&sync.matched_clip_path);
        if !mapping_still_points_here || !still_the_matched_clip {
            state.rkbx_sync[slot] = None;
            continue;
        }
        let Some(dj_time) = state.rkbx_link.latest().deck_time[sync.dj_deck] else { continue };
        let actual_elapsed = sync.seek_seconds + sync.seeked_at.elapsed().as_secs_f64();
        if rkbx_drift_exceeds_threshold(dj_time, actual_elapsed) {
            state.rkbx_sync[slot] = Some(RkbxSyncState {
                dj_deck: sync.dj_deck,
                matched_clip_path: sync.matched_clip_path.clone(),
                seek_seconds: dj_time,
                seeked_at: now,
            });
        }
    }

    // Per-deck video decode: reconcile-then-upload, one
    // slot at a time, mirroring the global block just above. Runs
    // unconditionally every tick, regardless of that slot's
    // bus/opacity: a deck-video slot's decode/beat-cut/warp must
    // never gate on visibility; the projectM `InvisibleMode`
    // throttle just below only applies to preset-mode slots.
    for slot in 0..deck::DECK_COUNT {
        let mut desired_slot = desired_video_input(&state.show.deck_video[slot], &state.video_clips, false);
        // Apply the sync's seek offset when this slot's
        // desired input is still the clip the match loaded.
        if let (Some(opendrop_io::video_capture::VideoInput::File { path, start_seconds }), Some(sync)) =
            (&mut desired_slot, &state.rkbx_sync[slot])
        {
            if *path == sync.matched_clip_path {
                *start_seconds = sync.seek_seconds;
            }
        }
        tick_deck_video(
            &state.gl,
            desired_slot,
            &mut state.deck_video_input[slot],
            &state.deck_video_capture[slot],
            &mut state.deck_video_frame_seq[slot],
            state.deck_video_texture[slot],
        );
    }
}

/// A new track started: remembers it as the current one and, when asked to,
/// brings back the preset last played on it. Runs for every reported track,
/// whether or not a video clip matches it.
fn on_track_changed(state: &mut AppState, artist: &str, title: &str) {
    let key = TrackMemory::key(artist, title);
    let first = state.track_memory.set_current_track(key).first().cloned();
    if let (true, Some(preset)) = (state.track_auto_recall, first) {
        if state.path_by_name.contains_key(&preset) {
            request_preset_load(state, state.show.selected_slot, preset);
        }
    }
}
