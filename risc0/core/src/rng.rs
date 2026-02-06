//! Xorshift32 PRNG - identical output in JS, Rust, and RISC-V.
//!
//! Algorithm: x ^= x << 13; x ^= x >> 17; x ^= x << 5;
//! The TypeScript version uses `>>> 0` to force unsigned 32-bit;
//! Rust u32 naturally wraps via `Wrapping` semantics.

#[derive(Debug, Clone)]
pub struct SeededRng {
    state: u32,
}

impl SeededRng {
    pub fn new(seed: u32) -> Self {
        // Must be non-zero; use default if 0 provided (matches TS: `seed >>> 0 || 0xdeadbeef`)
        let state = if seed == 0 { 0xDEADBEEF } else { seed };
        Self { state }
    }

    pub fn get_state(&self) -> u32 {
        self.state
    }

    /// Generate next random u32.
    pub fn next(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        self.state
    }

    /// Random i32 in [min, max_exclusive).
    /// Matches TypeScript `randomInt(min, maxExclusive)`.
    pub fn next_range(&mut self, min: i32, max_exclusive: i32) -> i32 {
        let range = (max_exclusive - min) as u32;
        min + (self.next() % range) as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xorshift32_known_sequence() {
        // Verified against TypeScript: new SeededRng(0xDEADBEEF).next() x10
        let mut rng = SeededRng::new(0xDEADBEEF);
        assert_eq!(rng.next(), 1199382711);
        assert_eq!(rng.next(), 2384302402);
        assert_eq!(rng.next(), 3129746520);
        assert_eq!(rng.next(), 4276113467);
        assert_eq!(rng.next(), 1745748808);
        assert_eq!(rng.next(), 2760751131);
        assert_eq!(rng.next(), 1649732188);
        assert_eq!(rng.next(), 486387635);
        assert_eq!(rng.next(), 2289630710);
        assert_eq!(rng.next(), 1862841525);
        assert_eq!(rng.get_state(), 1862841525);
    }

    #[test]
    fn test_zero_seed_defaults() {
        let rng = SeededRng::new(0);
        assert_eq!(rng.get_state(), 0xDEADBEEF);
    }

    #[test]
    fn test_next_range() {
        let mut rng = SeededRng::new(42);
        for _ in 0..1000 {
            let val = rng.next_range(-10, 10);
            assert!(val >= -10 && val < 10, "got {val}");
        }
    }

    #[test]
    fn test_determinism() {
        let mut a = SeededRng::new(12345);
        let mut b = SeededRng::new(12345);
        for _ in 0..1000 {
            assert_eq!(a.next(), b.next());
        }
    }
}
