//! SplitMix64, written out, so a seed rebuilds the same bytes on any machine.
//!
//! `cargo test` runs the generator over hundreds of seeds, and a failure has to
//! name the seed that produced it and nothing else — no thread id, no clock, no
//! address, no `HashMap` order. Everything the generator draws comes from here,
//! so "reproducible from its seed" is a property of this file.
//!
//! The algorithm is the one Vigna publishes, and `edge_the_generator_is_the_
//! sequence_splitmix64_publishes` pins the first four outputs of seed 0 against
//! the reference stream rather than against this implementation's own first
//! four, so a mistake in the constants cannot agree with itself.

/// The generator.
#[derive(Debug, Clone)]
pub struct Rng {
    state: u64,
}

impl Rng {
    /// A generator that will produce the stream `seed` names.
    ///
    /// Seed 0 is a real seed, not a special case: `state` is the seed itself and
    /// the first draw adds the golden-ratio step before mixing, which is what
    /// makes stream 0 well spread rather than a run of near-zeros.
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// The next 64 bits.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number in `0..bound`.
    ///
    /// Returns 0 for an empty range rather than dividing by zero: `below(0)` is
    /// a caller's mistake, and the one thing it must not be is a panic on the
    /// floor of a number line.
    pub fn below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            return 0;
        }
        (self.next_u64() % bound as u64) as usize
    }

    /// A number in `low..=high`, inclusive at both ends.
    ///
    /// An inverted range yields `low`, so a table whose bounds are computed can
    /// never ask for a range that is not there.
    pub fn between(&mut self, low: i64, high: i64) -> i64 {
        if high <= low {
            return low;
        }
        let width = (high - low) as u64 + 1;
        low + (self.next_u64() % width) as i64
    }

    /// True one time in `one_in`.
    pub fn chance(&mut self, one_in: usize) -> bool {
        one_in > 0 && self.below(one_in) == 0
    }

    /// One element of `items`, or `None` for an empty table.
    ///
    /// Takes the table by reference and hands the reference back, so drawing
    /// does not clone a `Value`-shaped payload into every arm.
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        if items.is_empty() {
            return None;
        }
        let index = self.below(items.len());
        items.get(index)
    }
}

/// The seed the suite generates from when a test names no seed of its own.
///
/// Not zero: `edge_the_default_seed_is_not_zero` holds it, because a suite whose
/// default stream is stream 0 is a suite whose every failure is about the same
/// program.
pub const DEFAULT_SEED: u64 = 0x5242_4C55_4500_0001;

#[cfg(test)]
mod tests {
    use super::*;

    /// The published SplitMix64 stream for seed 0, from Vigna's reference
    /// implementation rather than from this file.
    #[test]
    fn edge_the_generator_is_the_sequence_splitmix64_publishes() {
        let mut rng = Rng::new(0);
        let got: Vec<u64> = (0..4).map(|_| rng.next_u64()).collect();
        assert_eq!(
            got,
            vec![
                0xE220_A839_7B1D_CDAF,
                0x6E78_9E6A_A1B9_65F4,
                0x06C4_5D18_8009_454F,
                0xF88B_B8A8_724C_81EC,
            ],
            "the generator must be SplitMix64 exactly, or a seed does not name a \
             program and no failure is reproducible",
        );
    }

    #[test]
    fn edge_a_seed_is_replayed_and_a_different_seed_is_not() {
        let first: Vec<u64> = {
            let mut rng = Rng::new(12345);
            (0..8).map(|_| rng.next_u64()).collect()
        };
        let again: Vec<u64> = {
            let mut rng = Rng::new(12345);
            (0..8).map(|_| rng.next_u64()).collect()
        };
        let other: Vec<u64> = {
            let mut rng = Rng::new(12346);
            (0..8).map(|_| rng.next_u64()).collect()
        };
        assert_eq!(first, again, "one seed must replay exactly");
        assert_ne!(first, other, "two seeds must not share a stream");
    }

    #[test]
    fn edge_the_bounded_helpers_stay_inside_their_bounds() {
        let mut rng = Rng::new(DEFAULT_SEED);
        let mut lows: Vec<i64> = Vec::new();
        let mut highs: Vec<i64> = Vec::new();
        let mut singles: Vec<i64> = Vec::new();
        for _ in 0..2_000 {
            let v = rng.between(-3, 5);
            assert!((-3..=5).contains(&v), "{v} is outside -3..=5");
            lows.push(v);
            assert!(rng.below(4) < 4, "below(4) must stay under 4");
            assert_eq!(rng.below(1), 0, "below(1) has exactly one answer");
            assert_eq!(rng.between(7, 7), 7, "an empty range is its low end");
            assert_eq!(rng.between(9, 2), 9, "an inverted range is its low end");
            assert!(!rng.chance(0), "zero is not a probability");
            highs.push(v);
            singles.push(v);
        }
        assert!(
            lows.contains(&-3) && highs.contains(&5),
            "2,000 draws over -3..=5 must reach both ends, or the bound is not \
             doing what it says",
        );
        assert_eq!(singles.len(), 2_000);
    }

    #[test]
    fn edge_a_draw_over_an_empty_table_is_none_and_not_a_panic() {
        let mut rng = Rng::new(1);
        let empty: [u8; 0] = [];
        assert_eq!(rng.pick(&empty), None, "an empty table has nothing to draw");
        assert_eq!(rng.below(0), 0, "an empty range must not divide by zero");
    }

    #[test]
    fn edge_a_flag_draws_both_answers() {
        let mut rng = Rng::new(DEFAULT_SEED);
        let mut trues = 0usize;
        for _ in 0..1_000 {
            if rng.chance(2) {
                trues += 1;
            }
        }
        assert!(
            (300..=700).contains(&trues),
            "chance(2) over 1,000 draws gave {trues} trues; a flag that only \
             answers one way is not a flag",
        );
    }

    #[test]
    fn edge_the_default_seed_is_not_zero() {
        assert_ne!(
            DEFAULT_SEED, 0,
            "every test that names no seed would then generate the same program",
        );
        let mut a = Rng::new(DEFAULT_SEED);
        let mut b = Rng::new(DEFAULT_SEED);
        assert_eq!(a.next_u64(), b.next_u64());
    }
}
