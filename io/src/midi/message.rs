//! MIDI byte-sequence parsing, mirroring `MidiEngine._handle` (OpenDrop-VJ
//! `src/lib/engine/midi.ts:135-192`) exactly. Pure logic: no I/O, no MIDI
//! hardware: consumes a device id and a raw MIDI byte slice, returns at most
//! one decoded event.
//!
//! A LATER task wraps [`MidiParser`] in a real `midir`-backed thread; this
//! module has no dependency on `midir` or any hardware/timer state.

use std::collections::HashMap;

/// A decoded MIDI event, as dispatched by [`MidiParser::handle`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MidiEvent {
    /// `0xF8` clock byte. Dispatched immediately; no further bytes are read.
    Clock,
    /// Pitchbend, 14-bit, center 8192. `value = (data[2] << 7) | data[1]`.
    Pitchbend { channel: u8, value: u16 },
    /// Control change. `value` is 7-bit (0-127) unless `is_14bit` is set, in
    /// which case it is 0-16383 and `number` is the coarse (MSB) CC number.
    Cc {
        channel: u8,
        number: u8,
        value: u16,
        is_14bit: bool,
    },
    /// Note on (`on: true`, `value > 0`) or note off (`on: false`).
    Note {
        channel: u8,
        number: u8,
        value: u8,
        on: bool,
    },
}

/// Parses raw MIDI byte sequences into [`MidiEvent`]s, holding the
/// in-progress 14-bit CC state (MSB received, awaiting its matching LSB).
///
/// The pending-MSB map is keyed by `(device_id, channel, coarse_number)`:
/// scoped per source device, matching the JS implementation's per-device
/// controller-input storage: so that two devices sending interleaved 14-bit
/// CCs on the same channel/number don't clobber each other's pending MSB.
#[derive(Debug, Default)]
pub struct MidiParser {
    pending_msb: HashMap<(String, u8, u8), u8>,
}

impl MidiParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Decodes one raw MIDI message. Returns `None` for messages this
    /// implementation doesn't handle (too short, or an unrecognized status
    /// type byte): mirrors every early `return` in the JS `_handle`.
    pub fn handle(&mut self, device_id: &str, data: &[u8]) -> Option<MidiEvent> {
        if data.is_empty() {
            return None;
        }
        if data[0] == 0xf8 {
            return Some(MidiEvent::Clock);
        }
        if data.len() < 2 {
            return None;
        }

        let status = data[0];
        let num = data[1];
        let val = if data.len() > 2 { data[2] } else { 0 };
        let type_byte = status & 0xf0;
        let channel = (status & 0x0f) + 1;

        if type_byte == 0xe0 {
            let value = ((val as u16) << 7) | (num as u16);
            return Some(MidiEvent::Pitchbend { channel, value });
        }

        if type_byte == 0xb0 {
            if num <= 31 {
                self.pending_msb
                    .insert((device_id.to_string(), channel, num), val);
                return Some(MidiEvent::Cc {
                    channel,
                    number: num,
                    value: val as u16,
                    is_14bit: false,
                });
            }
            if (32..=63).contains(&num) {
                let coarse = num - 32;
                let key = (device_id.to_string(), channel, coarse);
                if let Some(msb) = self.pending_msb.remove(&key) {
                    let value = ((msb as u16) << 7) | ((val as u16) & 0x7f);
                    return Some(MidiEvent::Cc {
                        channel,
                        number: coarse,
                        value,
                        is_14bit: true,
                    });
                }
                return Some(MidiEvent::Cc {
                    channel,
                    number: num,
                    value: val as u16,
                    is_14bit: false,
                });
            }
            return Some(MidiEvent::Cc {
                channel,
                number: num,
                value: val as u16,
                is_14bit: false,
            });
        }

        let on = match type_byte {
            0x90 => val > 0,
            0x80 => false,
            _ => return None,
        };
        Some(MidiEvent::Note {
            channel,
            number: num,
            value: val,
            on,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_byte_dispatches_immediately() {
        let mut p = MidiParser::new();
        assert_eq!(p.handle("dev", &[0xf8]), Some(MidiEvent::Clock));
    }

    #[test]
    fn empty_data_returns_none() {
        let mut p = MidiParser::new();
        assert_eq!(p.handle("dev", &[]), None);
    }

    #[test]
    fn truncated_non_clock_message_returns_none() {
        let mut p = MidiParser::new();
        assert_eq!(p.handle("dev", &[0x90]), None);
    }

    #[test]
    fn pitchbend_is_14bit_with_correct_byte_order() {
        // data[1] is the LSB, data[2] is the MSB: value = (msb << 7) | lsb.
        let mut p = MidiParser::new();
        let event = p.handle("dev", &[0xe0, 0x01, 0x40]);
        assert_eq!(
            event,
            Some(MidiEvent::Pitchbend {
                channel: 1,
                value: 8193, // (0x40 << 7) | 0x01
            })
        );
    }

    #[test]
    fn pitchbend_center_value() {
        let mut p = MidiParser::new();
        let event = p.handle("dev", &[0xe3, 0x00, 0x40]);
        assert_eq!(
            event,
            Some(MidiEvent::Pitchbend {
                channel: 4,
                value: 8192,
            })
        );
    }

    #[test]
    fn cc_7bit_standalone_below_32() {
        let mut p = MidiParser::new();
        let event = p.handle("dev", &[0xb0, 10, 64]);
        assert_eq!(
            event,
            Some(MidiEvent::Cc {
                channel: 1,
                number: 10,
                value: 64,
                is_14bit: false,
            })
        );
    }

    #[test]
    fn cc_14bit_combines_msb_then_matching_lsb() {
        let mut p = MidiParser::new();
        // MSB (num=5) dispatches its own 7-bit event immediately, and is
        // stored pending.
        let msb_event = p.handle("dev", &[0xb0, 5, 100]);
        assert_eq!(
            msb_event,
            Some(MidiEvent::Cc {
                channel: 1,
                number: 5,
                value: 100,
                is_14bit: false,
            })
        );
        // LSB (num=32+5=37) combines with the pending MSB into a 14-bit
        // value, reported under the coarse number (5).
        let lsb_event = p.handle("dev", &[0xb0, 37, 50]);
        assert_eq!(
            lsb_event,
            Some(MidiEvent::Cc {
                channel: 1,
                number: 5,
                value: 12850, // (100 << 7) | 50
                is_14bit: true,
            })
        );
    }

    #[test]
    fn cc_lsb_without_pending_msb_falls_back_to_7bit() {
        let mut p = MidiParser::new();
        let event = p.handle("dev", &[0xb0, 45, 50]);
        assert_eq!(
            event,
            Some(MidiEvent::Cc {
                channel: 1,
                number: 45,
                value: 50,
                is_14bit: false,
            })
        );
    }

    #[test]
    fn cc_at_or_above_64_is_always_7bit() {
        let mut p = MidiParser::new();
        let event = p.handle("dev", &[0xb0, 70, 99]);
        assert_eq!(
            event,
            Some(MidiEvent::Cc {
                channel: 1,
                number: 70,
                value: 99,
                is_14bit: false,
            })
        );
    }

    #[test]
    fn note_on() {
        let mut p = MidiParser::new();
        let event = p.handle("dev", &[0x90, 60, 100]);
        assert_eq!(
            event,
            Some(MidiEvent::Note {
                channel: 1,
                number: 60,
                value: 100,
                on: true,
            })
        );
    }

    #[test]
    fn note_off_via_zero_velocity_on_note_on_status() {
        let mut p = MidiParser::new();
        let event = p.handle("dev", &[0x90, 60, 0]);
        assert_eq!(
            event,
            Some(MidiEvent::Note {
                channel: 1,
                number: 60,
                value: 0,
                on: false,
            })
        );
    }

    #[test]
    fn note_off_via_0x80_status() {
        let mut p = MidiParser::new();
        let event = p.handle("dev", &[0x80, 60, 100]);
        assert_eq!(
            event,
            Some(MidiEvent::Note {
                channel: 1,
                number: 60,
                value: 100,
                on: false,
            })
        );
    }
}
