/// Xorshift32 RNG - deterministic, ZK-friendly
/// Uses only 32-bit integer operations: XOR and bit shifts
/// Identical output in JS, Rust, and RISC-V
#[derive(Clone, Copy, Debug)]
pub struct Rng {
    state: u32,
}

impl Rng {
    /// Create new RNG with seed
    pub fn new(seed: u32) -> Self {
        // Must be non-zero; use default if 0 provided
        let state = if seed == 0 { 0xdeadbeef } else { seed };
        Rng { state }
    }

    /// Get current state
    pub fn state(&self) -> u32 {
        self.state
    }

    /// Generate next random u32 using Xorshift32 algorithm
    pub fn next(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x.wrapping_shl(13);
        x ^= x.wrapping_shr(17);
        x ^= x.wrapping_shl(5);
        self.state = x;
        x
    }

    /// Random integer in [0, max)
    pub fn next_int(&mut self, max: u32) -> u32 {
        if max == 0 {
            return 0;
        }
        self.next() % max
    }

    /// Random integer in [min, max)
    pub fn next_range(&mut self, min: u32, max: u32) -> u32 {
        if min >= max {
            return min;
        }
        min + self.next_int(max - min)
    }

    /// Random boolean with given probability (0-1)
    /// probability is in Q8.8 format (0-256)
    pub fn next_bool_q8_8(&mut self, probability_q8_8: u16) -> bool {
        let threshold = (probability_q8_8 as u32) << 16; // Convert to Q24.8 for comparison
        let random = self.next() >> 8; // Use top 24 bits
        random < threshold
    }

    /// Random BAM angle [0, 256)
    pub fn next_angle(&mut self) -> u8 {
        (self.next() & 0xFF) as u8
    }

    /// Random spin in [-3, 4)
    pub fn next_spin(&mut self) -> i8 {
        (self.next_range(0, 7) as i8) - 3
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xorshift32_sequence() {
        let mut rng = Rng::new(12345);

        // First few values should match known Xorshift32 sequence
        let first = rng.next();
        assert_eq!(first, 3299914889);

        let second = rng.next();
        assert_eq!(second, 1827393881);

        let third = rng.next();
        assert_eq!(third, 3883696615);
    }

    #[test]
    fn test_next_int() {
        let mut rng = Rng::new(12345);

        for _ in 0..100 {
            let val = rng.next_int(100);
            assert!(val < 100);
        }
    }

    #[test]
    fn test_next_range() {
        let mut rng = Rng::new(12345);

        for _ in 0..100 {
            let val = rng.next_range(10, 20);
            assert!(val >= 10 && val < 20);
        }
    }

    #[test]
    fn test_nonzero_seed() {
        // Zero seed should use default
        let rng1 = Rng::new(0);
        let rng2 = Rng::new(0xdeadbeef);
        assert_eq!(rng1.state(), rng2.state());
    }
}
