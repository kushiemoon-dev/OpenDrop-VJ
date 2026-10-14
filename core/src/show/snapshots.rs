use super::*;

impl Show {
    /// Reads back the current value of a snapshot-capturable `CommandId`:
    /// the inverse of the `CommandContext::set_*` setters those same ids
    /// dispatch to. `None` for any `CommandId` outside
    /// `SNAPSHOT_CAPTURABLE_IDS`, whether because it has no real setter yet
    /// or because it is deliberately excluded: the `CommandId::Time*` and
    /// `CommandId::Qvar*` families do have real setters and
    /// still return `None` here on purpose; see `SNAPSHOT_CAPTURABLE_IDS`
    /// for why.
    pub fn get_command_value(&self, id: CommandId) -> Option<f64> {
        match id {
            CommandId::ColorHueA => Some(self.color_params_a.hue_rotate),
            CommandId::ColorSatA => Some(self.color_params_a.saturate),
            CommandId::ColorBrightA => Some(self.color_params_a.brightness),
            CommandId::ColorContrastA => Some(self.color_params_a.contrast),
            CommandId::ColorInvertA => Some(self.color_params_a.invert),
            CommandId::ColorHueB => Some(self.color_params_b.hue_rotate),
            CommandId::ColorSatB => Some(self.color_params_b.saturate),
            CommandId::ColorBrightB => Some(self.color_params_b.brightness),
            CommandId::ColorContrastB => Some(self.color_params_b.contrast),
            CommandId::ColorInvertB => Some(self.color_params_b.invert),
            CommandId::CompositeBlend0 => Some(blend_mode_to_value01(self.slot_composites[0].blend)),
            CommandId::CompositeBlend1 => Some(blend_mode_to_value01(self.slot_composites[1].blend)),
            CommandId::CompositeBlend2 => Some(blend_mode_to_value01(self.slot_composites[2].blend)),
            CommandId::CompositeBlend3 => Some(blend_mode_to_value01(self.slot_composites[3].blend)),
            CommandId::LumakeyBlack0 => Some(self.slot_composites[0].luma_black),
            CommandId::LumakeyBlack1 => Some(self.slot_composites[1].luma_black),
            CommandId::LumakeyBlack2 => Some(self.slot_composites[2].luma_black),
            CommandId::LumakeyBlack3 => Some(self.slot_composites[3].luma_black),
            CommandId::LumakeyWhite0 => Some(self.slot_composites[0].luma_white),
            CommandId::LumakeyWhite1 => Some(self.slot_composites[1].luma_white),
            CommandId::LumakeyWhite2 => Some(self.slot_composites[2].luma_white),
            CommandId::LumakeyWhite3 => Some(self.slot_composites[3].luma_white),
            CommandId::ColorkeyHue0 => Some(self.slot_composites[0].color_hue),
            CommandId::ColorkeyHue1 => Some(self.slot_composites[1].color_hue),
            CommandId::ColorkeyHue2 => Some(self.slot_composites[2].color_hue),
            CommandId::ColorkeyHue3 => Some(self.slot_composites[3].color_hue),
            CommandId::ColorkeyTolerance0 => Some(self.slot_composites[0].color_tol),
            CommandId::ColorkeyTolerance1 => Some(self.slot_composites[1].color_tol),
            CommandId::ColorkeyTolerance2 => Some(self.slot_composites[2].color_tol),
            CommandId::ColorkeyTolerance3 => Some(self.slot_composites[3].color_tol),
            _ => None,
        }
    }

    /// Captures the current value of every snapshot-capturable `CommandId`
    /// (`SNAPSHOT_CAPTURABLE_IDS`): what the snapshot panel's Save button
    /// stores into a slot.
    pub fn capture_snapshot_values(&self) -> HashMap<CommandId, f64> {
        SNAPSHOT_CAPTURABLE_IDS.iter().filter_map(|&id| self.get_command_value(id).map(|v| (id, v))).collect()
    }

    /// Advances an in-progress snapshot recall by `dt_sec` (same
    /// caller-supplied-dt convention as `tick_playlists`) and returns the
    /// interpolated `(CommandId, value)` pairs for this tick. Dispatching
    /// them is the caller's job (`app::about_to_wait`, through
    /// `CommandRegistry::dispatch`) so a recall stays on the same
    /// keyboard/MIDI/OSC/remote-ws parity path as every other command.
    /// Returns an empty `Vec` when no recall is active, or when the target
    /// slot was cleared mid-recall (which also clears `active_recall`).
    pub fn tick_recall(&mut self, dt_sec: f64) -> Vec<(CommandId, f64)> {
        let Some(active) = &self.active_recall else { return Vec::new() };
        let slot = active.slot;
        let Some(snapshot) = &self.snapshot_slots[slot] else {
            self.active_recall = None;
            return Vec::new();
        };
        let (values, next) = tick_active_recall(active, &snapshot.values, self.snapshot_recall_duration_sec, dt_sec);
        self.active_recall = next;
        values.into_iter().collect()
    }

    /// Advances timeline playback by `dt_sec` (same caller-supplied-dt
    /// convention as `tick_recall`/`tick_playlists` above) and returns the
    /// interpolated `(CommandId, value)` pairs for this tick: dispatching
    /// them is the caller's job (`app::about_to_wait`, through
    /// `CommandRegistry::dispatch`), same keyboard/MIDI/OSC/remote-ws
    /// parity reasoning as `tick_recall`. Returns an empty `Vec` when not
    /// playing, or when there are fewer than 2 keyframes (nothing to loop
    /// over: `timeline_loop_duration` is 0 in that case).
    pub fn tick_timeline(&mut self, dt_sec: f64) -> Vec<(CommandId, f64)> {
        if !self.timeline_playing {
            return Vec::new();
        }
        let duration = timeline_loop_duration(&self.timeline_keyframes);
        if duration <= 0.0 {
            return Vec::new();
        }
        self.timeline_elapsed_sec += dt_sec;
        let t = self.timeline_elapsed_sec % duration;
        timeline_values_at(&self.timeline_keyframes, &self.snapshot_slots, t).into_iter().collect()
    }
}

/// `CommandId`s the snapshot panel's Save button captures and a recall
/// interpolates toward: the Color and Composite setters. See
/// `Show::get_command_value`.
///
/// This is a curated subset, not "every setter that exists yet": having a
/// real `CommandContext` setter is necessary but not sufficient, and the
/// list is not expected to keep growing with each new one. 221 of the 223
/// commands have a setter and only these 30 are captured: the two large
/// families left out are excluded permanently and by design, for the
/// reasons below.
///
/// **The 32 `CommandId::Time*` setters are deliberately left out**,
/// rather than overlooked. Two reasons:
///
/// - *Bandwidth.* Time values reach the decks through a side channel that
///   carries one value per deck per frame (see `engine::preset_patch`). A
///   recall interpolates every captured id every frame, so adding Time would
///   put up to 7 continuously-changing values per deck on a 1-per-frame
///   channel: each would land at roughly 1/7 the recall's own rate, making a
///   recall visibly step rather than glide, and starving whatever else was
///   already moving. Every id currently in this list is applied host-side by
///   the GPU compositor, with no such ceiling.
/// - *Round-tripping the scale.* Everything here is stored 0..1, which is
///   also the `value01` a recall dispatches. Time is stored 0..2 (see
///   `time_params::TIME_MULT_MAX`), so `get_command_value` would have to
///   divide by `TIME_MULT_MAX` to invert `commands::time_mult` exactly:
///   easy to get wrong, and silently wrong if it ever is, since a snapshot
///   would recall to half or double what was saved.
///
/// **The 128 `CommandId::Qvar*` setters are left out for the same
/// two reasons**, both of which apply at least as strongly: they share the
/// very same one-value-per-deck-per-frame channel (so a captured Qvar would
/// compete with Time *and* with the other 31 watches), and they are stored
/// -2..2 rather than 0..1, so the same inversion of `commands::q_var_value`
/// would have to be exact. A third reason is specific to Qvar: dispatching
/// one *enables* the watch it addresses, so a recall would silently switch
/// watches on, and switching a watch on re-patches and reloads the deck's
/// preset: a recall that did that every frame would be a reload loop.
///
/// Consequence to be aware of: saving a snapshot captures neither Time nor
/// Qvar, and recalling one leaves both where they are.
pub(super) const SNAPSHOT_CAPTURABLE_IDS: [CommandId; 30] = [
    CommandId::ColorHueA,
    CommandId::ColorSatA,
    CommandId::ColorBrightA,
    CommandId::ColorContrastA,
    CommandId::ColorInvertA,
    CommandId::ColorHueB,
    CommandId::ColorSatB,
    CommandId::ColorBrightB,
    CommandId::ColorContrastB,
    CommandId::ColorInvertB,
    CommandId::CompositeBlend0,
    CommandId::CompositeBlend1,
    CommandId::CompositeBlend2,
    CommandId::CompositeBlend3,
    CommandId::LumakeyBlack0,
    CommandId::LumakeyBlack1,
    CommandId::LumakeyBlack2,
    CommandId::LumakeyBlack3,
    CommandId::LumakeyWhite0,
    CommandId::LumakeyWhite1,
    CommandId::LumakeyWhite2,
    CommandId::LumakeyWhite3,
    CommandId::ColorkeyHue0,
    CommandId::ColorkeyHue1,
    CommandId::ColorkeyHue2,
    CommandId::ColorkeyHue3,
    CommandId::ColorkeyTolerance0,
    CommandId::ColorkeyTolerance1,
    CommandId::ColorkeyTolerance2,
    CommandId::ColorkeyTolerance3,
];
