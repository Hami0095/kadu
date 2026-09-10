//! A single PCG32 PRNG owned by the simulation. Agents cannot read or influence it.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pcg32 {
    state: u64,
    inc: u64,
}

const MULTIPLIER: u64 = 6364136223846793005;

impl Pcg32 {
    pub fn new(seed: u64, seq: u64) -> Pcg32 {
        let mut rng = Pcg32 { state: 0, inc: (seq << 1) | 1 };
        rng.step();
        rng.state = rng.state.wrapping_add(seed);
        rng.step();
        rng
    }

    fn step(&mut self) {
        self.state = self.state.wrapping_mul(MULTIPLIER).wrapping_add(self.inc);
    }

    /// Next 32-bit output.
    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.step();
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    /// Uniform integer in [0, bound).
    pub fn next_bounded(&mut self, bound: u32) -> u32 {
        if bound == 0 {
            return 0;
        }
        let threshold = bound.wrapping_neg() % bound;
        loop {
            let r = self.next_u32();
            if r >= threshold {
                return r % bound;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_sequence() {
        let mut a = Pcg32::new(42, 1);
        let mut b = Pcg32::new(42, 1);
        for _ in 0..100 {
            assert_eq!(a.next_u32(), b.next_u32());
        }
    }

    #[test]
    fn different_seeds_diverge() {
        let mut a = Pcg32::new(1, 1);
        let mut b = Pcg32::new(2, 1);
        let seq_a: Vec<u32> = (0..10).map(|_| a.next_u32()).collect();
        let seq_b: Vec<u32> = (0..10).map(|_| b.next_u32()).collect();
        assert_ne!(seq_a, seq_b);
    }

    #[test]
    fn bounded_within_range() {
        let mut r = Pcg32::new(7, 3);
        for _ in 0..1000 {
            let v = r.next_bounded(17);
            assert!(v < 17);
        }
    }
}
