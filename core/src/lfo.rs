//! Port of OpenDrop-VJ `src/lib/engine/lfo.ts`: per-slot LFO modulation engine.
//!
//! `LFO_SLOTS` is fixed at 4. The TS `tick()` reallocates via `slots.map(...)`
//! every call: one heap allocation per rendered frame at 60Hz. This port
//! returns a stack-allocated `[LfoOutput; LFO_SLOTS]` instead, eliminating
//! that per-frame allocation while keeping identical output semantics.

use crate::commands::CommandId;
use crate::rng::Xorshift64;

const LFO_SLOTS: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LfoShape {
    Sine,
    Saw,
    Square,
    Sh,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LfoSlot {
    pub enabled: bool,
    pub shape: LfoShape,
    /// Multiplier of beat rate: 1 = once per beat, 2 = twice per beat, 0.5 = once per 2 beats
    pub rate: f64,
    /// Phase offset 0..1
    pub offset: f64,
    /// Center of the modulation range 0..1
    pub center: f64,
    /// Modulation depth 0..1 (peak deviation from center)
    pub amount: f64,
    /// Target command id (must be 'range' kind). None = no routing.
    pub target: Option<CommandId>,
}

impl Default for LfoSlot {
    fn default() -> Self {
        Self {
            enabled: false,
            shape: LfoShape::Sine,
            rate: 1.0,
            offset: 0.0,
            center: 0.5,
            amount: 0.5,
            target: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LfoOutput {
    pub target: Option<CommandId>,
    pub value01: f64,
}

pub struct LfoEngine {
    pub slots: [LfoSlot; LFO_SLOTS],
    /// S&H values: randomized per-slot on each downbeat.
    sh_values: [f64; LFO_SLOTS],
    rng: Xorshift64,
}

impl LfoEngine {
    pub fn new() -> Self {
        Self {
            slots: [LfoSlot::default(); LFO_SLOTS],
            sh_values: [0.5; LFO_SLOTS],
            rng: Xorshift64::default(),
        }
    }

    /// Reseeds the S&H RNG with real per-launch entropy supplied by the
    /// caller (`core` stays zero-I/O and has no clock of its own). See
    /// `rng.rs`'s module doc comment: whole-branch review Finding I4.
    /// Wired since Step 11 of the Phase 8 VJ-panels plan: `Show::reseed_rng`
    /// (`show.rs`) calls this alongside the two playlist engines and the
    /// overlay/video shuffles, and `Show::on_beat` calls
    /// [`LfoEngine::randomize_sh`] on each downbeat.
    pub fn reseed_rng(&mut self, seed: u64) {
        self.rng.reseed(seed);
    }

    /// Call on each downbeat (beat 0 mod N) to refresh S&H samples.
    pub fn randomize_sh(&mut self) {
        for v in self.sh_values.iter_mut() {
            *v = self.rng.next_f64();
        }
    }

    /// Compute all LFO values for the given clock phase (0..1 within a beat).
    pub fn tick(&self, clock_phase01: f64) -> [LfoOutput; LFO_SLOTS] {
        std::array::from_fn(|i| {
            let slot = self.slots[i];
            if slot.enabled {
                LfoOutput {
                    target: slot.target,
                    value01: self.compute(&slot, clock_phase01, self.sh_values[i]),
                }
            } else {
                LfoOutput { target: None, value01: slot.center }
            }
        })
    }

    fn compute(&self, slot: &LfoSlot, clock_phase: f64, sh_value: f64) -> f64 {
        let p = (clock_phase * slot.rate + slot.offset) % 1.0;
        let raw = match slot.shape {
            LfoShape::Sine => ((p * std::f64::consts::TAU).sin() + 1.0) / 2.0,
            LfoShape::Saw => p,
            LfoShape::Square => if p < 0.5 { 1.0 } else { 0.0 },
            LfoShape::Sh => sh_value,
        };
        let value = slot.center + (raw - 0.5) * slot.amount;
        value.clamp(0.0, 1.0)
    }
}

impl Default for LfoEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tick_first(engine: &LfoEngine, phase: f64) -> LfoOutput {
        engine.tick(phase)[0]
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
    }

    mod sine {
        use super::*;

        #[test]
        fn equals_0_5_at_phase_0() {
            let mut engine = LfoEngine::new();
            engine.slots[0] = LfoSlot {
                enabled: true,
                shape: LfoShape::Sine,
                center: 0.5,
                amount: 1.0,
                rate: 1.0,
                ..Default::default()
            };
            assert_close(tick_first(&engine, 0.0).value01, 0.5);
        }

        #[test]
        fn equals_1_at_phase_0_25_peak() {
            let mut engine = LfoEngine::new();
            engine.slots[0] = LfoSlot {
                enabled: true,
                shape: LfoShape::Sine,
                center: 0.5,
                amount: 1.0,
                rate: 1.0,
                ..Default::default()
            };
            assert_close(tick_first(&engine, 0.25).value01, 1.0);
        }

        #[test]
        fn equals_0_at_phase_0_75_trough() {
            let mut engine = LfoEngine::new();
            engine.slots[0] = LfoSlot {
                enabled: true,
                shape: LfoShape::Sine,
                center: 0.5,
                amount: 1.0,
                rate: 1.0,
                ..Default::default()
            };
            assert_close(tick_first(&engine, 0.75).value01, 0.0);
        }
    }

    mod saw {
        use super::*;

        #[test]
        fn with_center_0_5_amount_1_gives_range_0_1() {
            let mut engine = LfoEngine::new();
            engine.slots[0] = LfoSlot {
                enabled: true,
                shape: LfoShape::Saw,
                center: 0.5,
                amount: 1.0,
                rate: 1.0,
                ..Default::default()
            };
            assert_close(tick_first(&engine, 0.6).value01, 0.6);
            assert_close(tick_first(&engine, 0.0).value01, 0.0);
            assert_close(tick_first(&engine, 1.0).value01, 0.0);
        }
    }

    mod square {
        use super::*;

        #[test]
        fn equals_1_in_the_first_half() {
            let mut engine = LfoEngine::new();
            engine.slots[0] = LfoSlot {
                enabled: true,
                shape: LfoShape::Square,
                center: 0.5,
                amount: 1.0,
                rate: 1.0,
                ..Default::default()
            };
            assert_eq!(tick_first(&engine, 0.25).value01, 1.0);
        }

        #[test]
        fn equals_0_in_the_second_half() {
            let mut engine = LfoEngine::new();
            engine.slots[0] = LfoSlot {
                enabled: true,
                shape: LfoShape::Square,
                center: 0.5,
                amount: 1.0,
                rate: 1.0,
                ..Default::default()
            };
            assert_eq!(tick_first(&engine, 0.75).value01, 0.0);
        }
    }

    mod rate {
        use super::*;

        #[test]
        fn rate_2_doubles_the_frequency_full_cycle_at_phase_0_5() {
            let mut engine = LfoEngine::new();
            engine.slots[0] = LfoSlot {
                enabled: true,
                shape: LfoShape::Saw,
                center: 0.5,
                amount: 1.0,
                rate: 2.0,
                ..Default::default()
            };
            assert_close(tick_first(&engine, 0.5).value01, 0.0);
        }
    }

    mod amount_and_center {
        use super::*;

        #[test]
        fn amount_0_5_limits_the_deviation_to_plus_minus_0_25_of_center() {
            let mut engine = LfoEngine::new();
            engine.slots[0] = LfoSlot {
                enabled: true,
                shape: LfoShape::Sine,
                center: 0.5,
                amount: 0.5,
                rate: 1.0,
                ..Default::default()
            };
            let peak = tick_first(&engine, 0.25).value01;
            let trough = tick_first(&engine, 0.75).value01;
            assert_close(peak, 0.75);
            assert_close(trough, 0.25);
        }

        #[test]
        fn values_clamped_to_0_1() {
            let mut engine = LfoEngine::new();
            engine.slots[0] = LfoSlot {
                enabled: true,
                shape: LfoShape::Sine,
                center: 0.0,
                amount: 2.0,
                rate: 1.0,
                ..Default::default()
            };
            for phase in [0.0, 0.25, 0.5, 0.75] {
                let v = tick_first(&engine, phase).value01;
                assert!(v >= 0.0);
                assert!(v <= 1.0);
            }
        }
    }

    mod disabled_slot {
        use super::*;

        #[test]
        fn returns_center_as_value_target_none() {
            let mut engine = LfoEngine::new();
            engine.slots[0] = LfoSlot {
                enabled: false,
                center: 0.3,
                target: Some(CommandId::Crossfader),
                ..Default::default()
            };
            let result = tick_first(&engine, 0.5);
            assert_eq!(result.target, None);
            assert_close(result.value01, 0.3);
        }
    }

    mod randomize_sh {
        use super::*;

        #[test]
        fn s_and_h_returns_the_memorized_value() {
            let mut engine = LfoEngine::new();
            engine.slots[0] = LfoSlot {
                enabled: true,
                shape: LfoShape::Sh,
                center: 0.5,
                amount: 1.0,
                ..Default::default()
            };
            engine.randomize_sh();
            let v1 = tick_first(&engine, 0.0).value01;
            let v2 = tick_first(&engine, 0.5).value01;
            assert_eq!(v1, v2);
        }
    }
}
