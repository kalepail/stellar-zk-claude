#[derive(Clone, Copy, Debug)]
pub struct SeededRng {
    state: u32,
}

impl SeededRng {
    pub fn new(seed: u32) -> Self {
        Self {
            state: if seed == 0 { 0xDEAD_BEEF } else { seed },
        }
    }

    pub fn state(&self) -> u32 {
        self.state
    }

    pub fn next(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        self.state
    }

    pub fn next_int(&mut self, max: u32) -> u32 {
        self.next() % max
    }

    pub fn next_range(&mut self, min: i32, max_exclusive: i32) -> i32 {
        debug_assert!(max_exclusive > min);
        let span = (max_exclusive - min) as u32;
        min + self.next_int(span) as i32
    }
}
