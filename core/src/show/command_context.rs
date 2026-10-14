use super::*;

impl CommandContext for Show {
    fn get_crossfader(&self) -> f64 {
        self.crossfader
    }

    fn set_crossfader(&mut self, v: f64) {
        self.crossfader = v.clamp(0.0, 1.0);
    }

    fn get_active_deck(&self) -> Deck {
        self.active_deck
    }

    fn switch_active_deck(&mut self) {
        self.active_deck = match self.active_deck {
            Deck::A => Deck::B,
            Deck::B => Deck::A,
        };
        // Keep `selected_slot` (what the
        // Decks panel highlights) in sync on a keyboard-driven switch too.
        // `select_slot` already derives `active_deck` from a clicked
        // slot's bus; without this, the reverse direction let the
        // highlighted card and the deck `PresetNextActive`/
        // `PlaylistNextActive` actually act on disagree. A deck whose bus
        // isn't assigned to any slot (both `Off`, or both slots on the
        // other letter) leaves `selected_slot` where it was, same
        // "no visible effect" contract `take_fired_presets` already has
        // for that case.
        if let Some(slot) = self.deck_bus_slot_for(self.active_deck) {
            self.selected_slot = slot;
        }
    }

    /// Cycles the deck's index through the full preset catalog (not the
    /// playlist, see `playlist::PlaylistEngine`) and reports the chosen
    /// name via `fired_preset_a`/`fired_preset_b`, drained by
    /// `take_fired_presets`. Port of `navigatePreset` in
    /// `+page.svelte:434-448`.
    fn navigate_preset(&mut self, deck: Deck, direction: i32) {
        if self.preset_catalog.is_empty() {
            return;
        }
        let len = self.preset_catalog.len();
        let idx_ref = match deck {
            Deck::A => &mut self.preset_index_a,
            Deck::B => &mut self.preset_index_b,
        };
        // Port of +page.svelte:436-439/441-444: avoids negative modulo.
        *idx_ref = if direction == 1 {
            (*idx_ref + 1) % len
        } else {
            ((if *idx_ref == 0 { len } else { *idx_ref }) - 1) % len
        };
        let name = self.preset_catalog[*idx_ref].name.clone();
        let cell = match deck {
            Deck::A => &self.fired_preset_a,
            Deck::B => &self.fired_preset_b,
        };
        *cell.borrow_mut() = Some(name);
    }

    fn toggle_playlist(&mut self, deck: Deck) {
        self.playlists.toggle_playlist(deck);
    }

    fn playlist_next(&mut self, deck: Deck) {
        self.playlists.playlist_next(deck);
    }

    fn playlist_prev(&mut self, deck: Deck) {
        self.playlists.playlist_prev(deck);
    }

    fn get_playlist_playing(&self, deck: Deck) -> bool {
        match deck {
            Deck::A => self.playlists.a_playing,
            Deck::B => self.playlists.b_playing,
        }
    }

    /// The overlay queue is real state now (`Show::overlay_store`), so
    /// this is no longer a no-op stub. `direction` is +1/-1 straight from
    /// `CommandId::OverlayQueueNext`/`Prev`; `advance_overlay_queue`
    /// itself treats anything that isn't +1 as "backwards", same as the
    /// TS source's `direction === 1 ? ... : ...`.
    fn advance_overlay_queue(&mut self, direction: i32) {
        self.overlay_store.advance_overlay_queue(direction);
    }

    fn set_color_hue_a(&mut self, v: f64) {
        self.color_params_a.hue_rotate = v.clamp(0.0, 1.0);
    }

    fn set_color_sat_a(&mut self, v: f64) {
        self.color_params_a.saturate = v.clamp(0.0, 1.0);
    }

    fn set_color_bright_a(&mut self, v: f64) {
        self.color_params_a.brightness = v.clamp(0.0, 1.0);
    }

    fn set_color_contrast_a(&mut self, v: f64) {
        self.color_params_a.contrast = v.clamp(0.0, 1.0);
    }

    fn set_color_invert_a(&mut self, v: f64) {
        self.color_params_a.invert = v.clamp(0.0, 1.0);
    }

    fn set_color_hue_b(&mut self, v: f64) {
        self.color_params_b.hue_rotate = v.clamp(0.0, 1.0);
    }

    fn set_color_sat_b(&mut self, v: f64) {
        self.color_params_b.saturate = v.clamp(0.0, 1.0);
    }

    fn set_color_bright_b(&mut self, v: f64) {
        self.color_params_b.brightness = v.clamp(0.0, 1.0);
    }

    fn set_color_contrast_b(&mut self, v: f64) {
        self.color_params_b.contrast = v.clamp(0.0, 1.0);
    }

    fn set_color_invert_b(&mut self, v: f64) {
        self.color_params_b.invert = v.clamp(0.0, 1.0);
    }

    fn set_time_speed(&mut self, slot: usize, v: f64) {
        self.time_params[slot].speed_mult = clamp_time_mult(v);
    }

    fn set_time_zoom(&mut self, slot: usize, v: f64) {
        self.time_params[slot].zoom_mult = clamp_time_mult(v);
    }

    fn set_time_rot(&mut self, slot: usize, v: f64) {
        self.time_params[slot].rot_mult = clamp_time_mult(v);
    }

    fn set_time_warp(&mut self, slot: usize, v: f64) {
        self.time_params[slot].warp_mult = clamp_time_mult(v);
    }

    fn set_time_dx(&mut self, slot: usize, v: f64) {
        self.time_params[slot].dx_mult = clamp_time_mult(v);
    }

    fn set_time_dy(&mut self, slot: usize, v: f64) {
        self.time_params[slot].dy_mult = clamp_time_mult(v);
    }

    fn set_time_stretch(&mut self, slot: usize, v: f64) {
        self.time_params[slot].stretch_mult = clamp_time_mult(v);
    }

    fn set_time_wave(&mut self, slot: usize, v: f64) {
        self.time_params[slot].wave_mult = clamp_time_mult(v);
    }

    /// Built from `q_vars`'s two ported helpers rather than indexing the
    /// arrays directly: they already carry the out-of-range guard the TS
    /// port grew (`slot > 3`, `n` outside 1..=32 is a no-op, not a panic),
    /// which matters here because `n`/`slot` come from a `CommandId` table
    /// and an OSC/remote-ws payload rather than from this crate. Order is
    /// load-bearing: `with_q_var_watch` resets the value to 0, so enabling
    /// has to happen *before* the value is written, not after.
    fn set_q_var(&mut self, slot: usize, n: usize, v: f64) {
        let enabled = with_q_var_watch(self.q_var_params, slot, n);
        self.q_var_params = with_q_var_value(enabled, slot, n, clamp_q_var_value(v));
    }

    fn set_composite_blend(&mut self, slot: usize, v: f64) {
        self.slot_composites[slot].blend = blend_mode_from_value01(v);
    }

    fn set_composite_luma_black(&mut self, slot: usize, v: f64) {
        self.slot_composites[slot].luma_black = v.clamp(0.0, 1.0);
    }

    fn set_composite_luma_white(&mut self, slot: usize, v: f64) {
        self.slot_composites[slot].luma_white = v.clamp(0.0, 1.0);
    }

    fn set_composite_color_hue(&mut self, slot: usize, v: f64) {
        self.slot_composites[slot].color_hue = v.clamp(0.0, 1.0);
    }

    fn set_composite_color_tol(&mut self, slot: usize, v: f64) {
        self.slot_composites[slot].color_tol = v.clamp(0.0, 1.0);
    }

    fn recall_snapshot(&mut self, slot: usize) {
        let Some(snapshot) = &self.snapshot_slots[slot] else { return };
        let start_values: HashMap<CommandId, f64> =
            snapshot.values.keys().filter_map(|&id| self.get_command_value(id).map(|v| (id, v))).collect();
        self.active_recall = Some(ActiveRecall { slot, start_values, elapsed_sec: 0.0 });
    }

    fn toggle_timeline(&mut self) {
        self.timeline_playing = !self.timeline_playing;
        if self.timeline_playing {
            self.timeline_elapsed_sec = 0.0;
        }
    }

    fn toggle_strobe(&mut self) {
        self.strobe.enabled = !self.strobe.enabled;
    }
}
