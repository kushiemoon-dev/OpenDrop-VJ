//! Shared xorshift64 PRNG used by shuffle-mode index selection
//! (`playlist.rs`, `overlay.rs`) and LFO sample-and-hold (`lfo.rs`).
//!
//! Whole-branch review Finding I4: these three call sites used to each carry
//! their own copy of the same xorshift64 algorithm, and `lfo.rs` used the
//! `rand` crate instead: a needless dependency in a crate whose whole point
//! is "zero I/O, testable to 100%" (`rand` pulls in `getrandom`/`libc`/
//! `chacha20`/etc). Consolidated here.
//!
//! `core` is zero-I/O and owns no entropy source of its own, so every
//! instance starts from a caller-supplied seed rather than a real random
//! one. [`Xorshift64::default`] uses a fixed, non-random seed purely so
//! existing callers/tests stay deterministic; a live `app` should call
//! [`Xorshift64::reseed`] once at bootstrap with real per-launch entropy
//! (e.g. derived from `std::time::SystemTime::now()`) so shuffle output
//! actually differs between launches instead of replaying the same sequence
//! every time.

/// A fixed, arbitrary starting state: NOT random. Used only as the default
/// seed so `Xorshift64::default()` (and every test that doesn't reseed
/// explicitly) stays deterministic and reproducible.
const DEFAULT_SEED: u64 = 0x9E3779B97F4A7C15;

#[derive(Debug, Clone, Copy)]
pub struct Xorshift64 {
    state: u64,
}

/// Warm-up rounds run once at construction so a low-entropy seed (small
/// integers like 1 or 2, adjacent seeds, etc.) has fully diffused into the
/// high bits `next_f64` reads before the first value is ever produced:
/// without this, two seeds differing only in a low bit can produce
/// indistinguishable output for several calls. Real callers (`app`,
/// deriving a seed from `SystemTime::now()`) won't hit this in practice, but
/// nothing here should depend on the caller's seed already being
/// well-mixed.
const WARMUP_ROUNDS: u32 = 16;

impl Xorshift64 {
    /// xorshift64 is undefined at state 0 (every subsequent step would also
    /// be 0), so a zero seed falls back to [`DEFAULT_SEED`] instead of
    /// silently producing a constant-0 stream forever.
    pub fn new(seed: u64) -> Self {
        let mut rng = Self { state: if seed == 0 { DEFAULT_SEED } else { seed } };
        for _ in 0..WARMUP_ROUNDS {
            rng.step();
        }
        rng
    }

    /// Restarts the sequence from `seed`: see the module doc comment for
    /// why a live `app` should call this once at bootstrap with real
    /// per-launch entropy.
    pub fn reseed(&mut self, seed: u64) {
        *self = Self::new(seed);
    }

    fn step(&mut self) {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
    }

    /// Next value in 0..1.
    pub fn next_f64(&mut self) -> f64 {
        self.step();
        (self.state >> 11) as f64 / (1u64 << 53) as f64
    }
}

impl Default for Xorshift64 {
    fn default() -> Self {
        Self::new(DEFAULT_SEED)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_produces_the_same_sequence() {
        let mut a = Xorshift64::new(42);
        let mut b = Xorshift64::new(42);
        for _ in 0..10 {
            assert_eq!(a.next_f64(), b.next_f64());
        }
    }

    #[test]
    fn different_seeds_produce_different_sequences() {
        let mut a = Xorshift64::new(1);
        let mut b = Xorshift64::new(2);
        let seq_a: Vec<f64> = (0..10).map(|_| a.next_f64()).collect();
        let seq_b: Vec<f64> = (0..10).map(|_| b.next_f64()).collect();
        assert_ne!(seq_a, seq_b);
    }

    #[test]
    fn reseed_restarts_the_sequence_from_the_new_seed() {
        let mut rng = Xorshift64::new(99);
        rng.next_f64();
        rng.next_f64();
        rng.reseed(99);
        let mut fresh = Xorshift64::new(99);
        assert_eq!(rng.next_f64(), fresh.next_f64());
    }

    #[test]
    fn a_zero_seed_falls_back_to_the_default_seed_instead_of_sticking_at_zero() {
        let mut rng = Xorshift64::new(0);
        // If the state had stayed 0, every subsequent xorshift step would too.
        assert_ne!(rng.next_f64(), 0.0);
    }

    #[test]
    fn adjacent_small_seeds_still_diverge_quickly() {
        // Regression test for the warm-up rounds: before they existed,
        // seed=1 and seed=2 (a realistic mistake for a caller passing a
        // low-entropy seed) produced indistinguishable index sequences for
        // several calls when scaled down to a small range, because their
        // low-order-bit difference hadn't yet diffused into the high bits
        // `next_f64` reads.
        let mut a = Xorshift64::new(1);
        let mut b = Xorshift64::new(2);
        let seq_a: Vec<usize> = (0..10).map(|_| (a.next_f64() * 5.0) as usize).collect();
        let seq_b: Vec<usize> = (0..10).map(|_| (b.next_f64() * 5.0) as usize).collect();
        assert_ne!(seq_a, seq_b);
    }

    #[test]
    fn values_stay_in_0_1() {
        let mut rng = Xorshift64::new(7);
        for _ in 0..100 {
            let v = rng.next_f64();
            assert!((0.0..1.0).contains(&v), "{v} out of range");
        }
    }
}
