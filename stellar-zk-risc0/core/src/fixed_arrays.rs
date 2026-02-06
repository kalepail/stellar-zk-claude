//! Performance-optimized game state using fixed-size arrays
//!
//! This module provides a stack-allocated alternative to Vec-based collections
//! to avoid expensive dynamic memory allocations in the zkVM.
//!
//! According to RISC0 research:
//! - Page-in/page-out costs ~1130 cycles
//! - Memory access is 1 cycle if already paged in
//! - Vec causes scattered memory and paging overhead

use crate::constants::*;
use crate::types::*;

/// Fixed-size bullet collection (max 4 player bullets)
#[derive(Clone, Debug)]
pub struct BulletArray {
    bullets: [Bullet; SHIP_BULLET_LIMIT as usize],
    count: u8,
}

impl Default for BulletArray {
    fn default() -> Self {
        Self {
            bullets: [Bullet::default(); SHIP_BULLET_LIMIT as usize],
            count: 0,
        }
    }
}

impl BulletArray {
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.count as usize
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    #[inline(always)]
    pub fn get(&self, index: usize) -> Option<&Bullet> {
        if index < self.count as usize {
            Some(&self.bullets[index])
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn get_mut(&mut self, index: usize) -> Option<&mut Bullet> {
        if index < self.count as usize {
            Some(&mut self.bullets[index])
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn push(&mut self, bullet: Bullet) {
        if self.count < SHIP_BULLET_LIMIT {
            self.bullets[self.count as usize] = bullet;
            self.count += 1;
        }
    }

    #[inline(always)]
    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(&Bullet) -> bool,
    {
        let mut write_idx = 0;
        for read_idx in 0..self.count as usize {
            if f(&self.bullets[read_idx]) {
                if write_idx != read_idx {
                    self.bullets[write_idx] = self.bullets[read_idx];
                }
                write_idx += 1;
            }
        }
        self.count = write_idx as u8;
    }

    #[inline(always)]
    pub fn iter(&self) -> impl Iterator<Item = &Bullet> {
        self.bullets[..self.count as usize].iter()
    }

    #[inline(always)]
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Bullet> {
        self.bullets[..self.count as usize].iter_mut()
    }

    #[inline(always)]
    pub fn clear(&mut self) {
        self.count = 0;
    }
}

/// Fixed-size asteroid collection (max 27 + 16 for splitting = 43 safe upper bound)
/// Actually: max 16 per wave + up to 32 from splitting = 48, but we cap at 48
pub const MAX_ASTEROIDS: usize = 48;

#[derive(Clone, Debug)]
pub struct AsteroidArray {
    asteroids: [Asteroid; MAX_ASTEROIDS],
    count: u8,
}

impl Default for AsteroidArray {
    fn default() -> Self {
        Self {
            asteroids: [Asteroid::default(); MAX_ASTEROIDS],
            count: 0,
        }
    }
}

impl AsteroidArray {
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.count as usize
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    #[inline(always)]
    pub fn get(&self, index: usize) -> Option<&Asteroid> {
        if index < self.count as usize {
            Some(&self.asteroids[index])
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn get_mut(&mut self, index: usize) -> Option<&mut Asteroid> {
        if index < self.count as usize {
            Some(&mut self.asteroids[index])
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn push(&mut self, asteroid: Asteroid) {
        if self.count < MAX_ASTEROIDS as u8 {
            self.asteroids[self.count as usize] = asteroid;
            self.count += 1;
        }
    }

    #[inline(always)]
    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(&Asteroid) -> bool,
    {
        let mut write_idx = 0;
        for read_idx in 0..self.count as usize {
            if f(&self.asteroids[read_idx]) {
                if write_idx != read_idx {
                    self.asteroids[write_idx] = self.asteroids[read_idx];
                }
                write_idx += 1;
            }
        }
        self.count = write_idx as u8;
    }

    #[inline(always)]
    pub fn iter(&self) -> impl Iterator<Item = &Asteroid> {
        self.asteroids[..self.count as usize].iter()
    }

    #[inline(always)]
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Asteroid> {
        self.asteroids[..self.count as usize].iter_mut()
    }

    #[inline(always)]
    pub fn clear(&mut self) {
        self.count = 0;
    }
}

/// Fixed-size saucer collection (max 3 for wave >= 7)
pub const MAX_SAUCERS: usize = 4; // Slight buffer

#[derive(Clone, Debug)]
pub struct SaucerArray {
    saucers: [Saucer; MAX_SAUCERS],
    count: u8,
}

impl Default for SaucerArray {
    fn default() -> Self {
        Self {
            saucers: [Saucer::default(); MAX_SAUCERS],
            count: 0,
        }
    }
}

impl SaucerArray {
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.count as usize
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    #[inline(always)]
    pub fn get(&self, index: usize) -> Option<&Saucer> {
        if index < self.count as usize {
            Some(&self.saucers[index])
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn get_mut(&mut self, index: usize) -> Option<&mut Saucer> {
        if index < self.count as usize {
            Some(&mut self.saucers[index])
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn push(&mut self, saucer: Saucer) {
        if self.count < MAX_SAUCERS as u8 {
            self.saucers[self.count as usize] = saucer;
            self.count += 1;
        }
    }

    #[inline(always)]
    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(&Saucer) -> bool,
    {
        let mut write_idx = 0;
        for read_idx in 0..self.count as usize {
            if f(&self.saucers[read_idx]) {
                if write_idx != read_idx {
                    self.saucers[write_idx] = self.saucers[read_idx];
                }
                write_idx += 1;
            }
        }
        self.count = write_idx as u8;
    }

    #[inline(always)]
    pub fn iter(&self) -> impl Iterator<Item = &Saucer> {
        self.saucers[..self.count as usize].iter()
    }

    #[inline(always)]
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Saucer> {
        self.saucers[..self.count as usize].iter_mut()
    }

    #[inline(always)]
    pub fn clear(&mut self) {
        self.count = 0;
    }
}

/// Optimized game state using fixed-size arrays
#[derive(Clone, Debug, Default)]
pub struct OptimizedGameState {
    pub mode: GameMode,
    pub frame_count: u32,
    pub score: u32,
    pub lives: u8,
    pub wave: u8,
    pub next_extra_life_score: u32,
    pub time_since_last_kill: u16,
    pub saucer_spawn_timer: u16,
    pub ship: Ship,
    pub bullets: BulletArray,
    pub asteroids: AsteroidArray,
    pub saucers: SaucerArray,
    pub saucer_bullets: BulletArray,
}

impl From<GameState> for OptimizedGameState {
    fn from(state: GameState) -> Self {
        let mut optimized = OptimizedGameState {
            mode: state.mode,
            frame_count: state.frame_count,
            score: state.score,
            lives: state.lives,
            wave: state.wave,
            next_extra_life_score: state.next_extra_life_score,
            time_since_last_kill: state.time_since_last_kill,
            saucer_spawn_timer: state.saucer_spawn_timer,
            ship: state.ship,
            bullets: BulletArray::default(),
            asteroids: AsteroidArray::default(),
            saucers: SaucerArray::default(),
            saucer_bullets: BulletArray::default(),
        };

        // Copy data from Vec to arrays
        for bullet in state.bullets {
            optimized.bullets.push(bullet);
        }
        for asteroid in state.asteroids {
            optimized.asteroids.push(asteroid);
        }
        for saucer in state.saucers {
            optimized.saucers.push(saucer);
        }
        for bullet in state.saucer_bullets {
            optimized.saucer_bullets.push(bullet);
        }

        optimized
    }
}

impl From<OptimizedGameState> for GameState {
    fn from(state: OptimizedGameState) -> Self {
        GameState {
            mode: state.mode,
            frame_count: state.frame_count,
            score: state.score,
            lives: state.lives,
            wave: state.wave,
            next_extra_life_score: state.next_extra_life_score,
            time_since_last_kill: state.time_since_last_kill,
            saucer_spawn_timer: state.saucer_spawn_timer,
            ship: state.ship,
            bullets: state.bullets.iter().copied().collect(),
            asteroids: state.asteroids.iter().copied().collect(),
            saucers: state.saucers.iter().copied().collect(),
            saucer_bullets: state.saucer_bullets.iter().copied().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bullet_array_operations() {
        let mut array = BulletArray::default();
        assert!(array.is_empty());

        array.push(Bullet::default());
        assert_eq!(array.len(), 1);

        array.push(Bullet::default());
        assert_eq!(array.len(), 2);

        // Test overflow protection
        for _ in 0..10 {
            array.push(Bullet::default());
        }
        assert_eq!(array.len(), 4); // Capped at limit

        // Test retain
        array.retain(|b| b.life > 0);
        // All default bullets have life = 0
        assert_eq!(array.len(), 0);
    }

    #[test]
    fn test_asteroid_array_operations() {
        let mut array = AsteroidArray::default();
        assert!(array.is_empty());

        for _ in 0..5 {
            array.push(Asteroid::default());
        }
        assert_eq!(array.len(), 5);

        // Test iteration
        let count = array.iter().count();
        assert_eq!(count, 5);
    }

    #[test]
    fn test_state_conversion() {
        let mut original = GameState::default();
        original.bullets.push(Bullet::default());
        original.asteroids.push(Asteroid::default());

        let optimized: OptimizedGameState = original.clone().into();
        assert_eq!(optimized.bullets.len(), 1);
        assert_eq!(optimized.asteroids.len(), 1);

        let converted: GameState = optimized.into();
        assert_eq!(converted.bullets.len(), 1);
        assert_eq!(converted.asteroids.len(), 1);
    }
}
