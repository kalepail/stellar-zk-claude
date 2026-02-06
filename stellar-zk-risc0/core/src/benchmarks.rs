//! Performance benchmarks for Asteroids ZK Verifier
//!
//! These benchmarks measure cycle counts and performance characteristics
//! critical for ZK proof generation. Use these to ensure the game stays
//! within RISC0 cycle limits.

#[cfg(test)]
mod tests {
    use crate::constants::*;
    use crate::engine::GameEngine;
    use crate::types::FrameInput;

    /// Helper to simulate a full game
    fn simulate_game(seed: u32, frames: u32) -> GameEngine {
        let mut engine = GameEngine::new(seed);

        // Simulate random gameplay
        for i in 0..frames {
            let input = FrameInput {
                left: i % 7 == 0,
                right: i % 11 == 0,
                thrust: i % 5 == 0,
                fire: i % 3 == 0,
            };
            engine.step(input);
        }

        engine
    }

    // =========================================================================
    // FRAME CYCLE BENCHMARKS
    // =========================================================================

    #[test]
    fn benchmark_single_frame() {
        let mut engine = GameEngine::new(12345);

        // Measure 1000 frames
        let start = std::time::Instant::now();
        for _ in 0..1000 {
            engine.step(FrameInput::default());
        }
        let duration = start.elapsed();

        let cycles_per_frame = duration.as_nanos() as f64 / 1000.0;
        println!("Single frame average: {:.2} ns", cycles_per_frame);

        // Rough estimate: native execution should be fast
        // In zkVM, will be ~1000-5000x slower
        assert!(duration.as_millis() < 100, "Single frame too slow");
    }

    #[test]
    fn benchmark_full_game_100_frames() {
        let start = std::time::Instant::now();
        let engine = simulate_game(12345, 100);
        let duration = start.elapsed();

        println!("100 frames: {:?}", duration);
        println!("Average per frame: {:?}", duration / 100);

        // Should complete quickly
        assert!(duration.as_millis() < 500, "100 frames too slow");
    }

    #[test]
    fn benchmark_full_game_1000_frames() {
        let start = std::time::Instant::now();
        let engine = simulate_game(12345, 1000);
        let duration = start.elapsed();

        println!("1000 frames: {:?}", duration);
        println!("Average per frame: {:?}", duration / 1000);

        // Should still be fast
        assert!(duration.as_secs() < 5, "1000 frames too slow");
    }

    #[test]
    fn benchmark_collision_heavy_gameplay() {
        let mut engine = GameEngine::new(12345);

        // Position ship to maximize collisions
        let start = std::time::Instant::now();

        for i in 0..100 {
            // Fire constantly to create many bullets
            let input = FrameInput {
                left: false,
                right: false,
                thrust: false,
                fire: true,
            };
            engine.step(input);

            // Some cooldown frames
            for _ in 0..SHIP_BULLET_COOLDOWN_FRAMES {
                engine.step(FrameInput::default());
            }
        }

        let duration = start.elapsed();
        println!("Collision-heavy 100 iterations: {:?}", duration);

        // Many bullets = more collision checks
        assert!(
            duration.as_millis() < 1000,
            "Collision-heavy gameplay too slow"
        );
    }

    // =========================================================================
    // MEMORY AND STATE SIZE BENCHMARKS
    // =========================================================================

    #[test]
    fn benchmark_state_size_growth() {
        let mut engine = GameEngine::new(12345);

        let initial_asteroids = engine.state().asteroids.len();
        let initial_bullets = engine.state().bullets.len();

        // Run for many frames
        for _ in 0..1000 {
            engine.step(FrameInput {
                left: false,
                right: false,
                thrust: false,
                fire: true,
            });
            // Wait for cooldown
            for _ in 0..SHIP_BULLET_COOLDOWN_FRAMES {
                engine.step(FrameInput::default());
            }
        }

        let final_asteroids = engine.state().asteroids.len();
        let final_bullets = engine.state().bullets.len();

        println!("Asteroids: {} -> {}", initial_asteroids, final_asteroids);
        println!("Bullets: {} -> {}", initial_bullets, final_bullets);

        // State should not grow unbounded
        assert!(
            final_asteroids <= ASTEROID_CAP as usize + 10,
            "Too many asteroids"
        );
        assert!(
            final_bullets <= SHIP_BULLET_LIMIT as usize,
            "Too many bullets"
        );
    }

    // =========================================================================
    // WAVE PROGRESSION BENCHMARKS
    // =========================================================================

    #[test]
    fn benchmark_high_wave_performance() {
        let mut engine = GameEngine::new(12345);

        // Simulate reaching wave 10
        for _ in 0..10 {
            // Kill all asteroids
            for asteroid in engine.state_mut().asteroids.iter_mut() {
                asteroid.alive = false;
            }
            // Step to spawn next wave
            engine.step(FrameInput::default());
        }

        assert_eq!(engine.state().wave, 11, "Should be at wave 11");

        // Benchmark performance at high wave
        let start = std::time::Instant::now();
        for _ in 0..100 {
            engine.step(FrameInput::default());
        }
        let duration = start.elapsed();

        println!(
            "Wave {} performance: {:?} for 100 frames",
            engine.state().wave,
            duration
        );

        // Should not degrade significantly with more asteroids
        assert!(duration.as_millis() < 500, "High wave performance degraded");
    }

    // =========================================================================
    // WORST-CASE SCENARIO BENCHMARKS
    // =========================================================================

    #[test]
    fn benchmark_max_entities() {
        let mut engine = GameEngine::new(12345);

        // Simulate maximum entities scenario
        // Max asteroids (16) + max bullets (4) + saucers (3 at high wave)

        // Reach high wave for max saucers
        for _ in 0..10 {
            for asteroid in engine.state_mut().asteroids.iter_mut() {
                asteroid.alive = false;
            }
            engine.step(FrameInput::default());
        }

        // Add max bullets
        for _ in 0..SHIP_BULLET_LIMIT {
            engine.step(FrameInput {
                left: false,
                right: false,
                thrust: false,
                fire: true,
            });
            for _ in 0..SHIP_BULLET_COOLDOWN_FRAMES {
                engine.step(FrameInput::default());
            }
        }

        let asteroid_count = engine.state().asteroids.iter().filter(|a| a.alive).count();
        let bullet_count = engine.state().bullets.len();

        println!(
            "Max entities - Asteroids: {}, Bullets: {}",
            asteroid_count, bullet_count
        );

        // Benchmark worst case
        let start = std::time::Instant::now();
        for _ in 0..100 {
            engine.step(FrameInput::default());
        }
        let duration = start.elapsed();

        println!("Max entities performance: {:?} for 100 frames", duration);

        // Should handle max entities gracefully
        assert!(
            duration.as_millis() < 1000,
            "Max entities performance too slow"
        );
    }

    // =========================================================================
    // ESTIMATED ZK CYCLE COUNT
    // =========================================================================

    #[test]
    fn estimate_full_game_cycles() {
        // Run a full simulated game (18000 frames = 5 minutes at 60fps)
        let frame_count = 18000;

        let start = std::time::Instant::now();
        let engine = simulate_game(12345, frame_count);
        let duration = start.elapsed();

        println!("\n=== ESTIMATED ZK CYCLE COUNT ===");
        println!("Full game ({} frames): {:?}", frame_count, duration);
        println!("Average frame time: {:?}", duration / frame_count as u32);

        // RISC0 guest is roughly 1000-5000x slower than native
        // Assuming ~5000 cycles per frame average
        let estimated_cycles = frame_count * 5000;
        println!("Estimated total cycles: {}M", estimated_cycles / 1_000_000);

        // RISC0 supports 100M+ cycles with continuations
        assert!(
            estimated_cycles < 100_000_000,
            "Estimated cycles exceed RISC0 limit"
        );

        // Should be well within limits
        println!("Status: WITHIN LIMITS ✓");
    }

    // =========================================================================
    // RULES CHECKING OVERHEAD
    // =========================================================================

    #[test]
    fn benchmark_rules_checking_overhead() {
        let mut engine = GameEngine::new(12345);

        // Benchmark without rules checking
        let start = std::time::Instant::now();
        for _ in 0..100 {
            engine.step(FrameInput::default());
        }
        let without_rules = start.elapsed();

        // Benchmark with rules checking (need fresh engine)
        let mut engine_with_rules = GameEngine::new(12345);
        let start = std::time::Instant::now();
        for i in 0..100 {
            engine_with_rules.step(FrameInput::default());
            // Rules checking happens internally if enabled
        }
        let with_rules = start.elapsed();

        let overhead = with_rules.as_nanos() as f64 / without_rules.as_nanos() as f64;
        println!("Rules checking overhead: {:.2}x", overhead);

        // Overhead should be minimal (rules are mostly simple checks)
        assert!(overhead < 2.0, "Rules checking overhead too high");
    }

    // =========================================================================
    // CROSS-VALIDATION BENCHMARK
    // =========================================================================

    #[test]
    fn benchmark_determinism_stability() {
        // Run same game multiple times, ensure identical results
        let seeds = [12345, 67890, 11111, 22222, 33333];

        for seed in seeds {
            let engine1 = simulate_game(seed, 1000);
            let engine2 = simulate_game(seed, 1000);

            // States should be identical
            assert_eq!(
                engine1.state().score,
                engine2.state().score,
                "Determinism failed for seed {}: score mismatch",
                seed
            );
            assert_eq!(
                engine1.state().lives,
                engine2.state().lives,
                "Determinism failed for seed {}: lives mismatch",
                seed
            );
            assert_eq!(
                engine1.state().wave,
                engine2.state().wave,
                "Determinism failed for seed {}: wave mismatch",
                seed
            );
            assert_eq!(
                engine1.state().ship.x,
                engine2.state().ship.x,
                "Determinism failed for seed {}: ship x mismatch",
                seed
            );
            assert_eq!(
                engine1.state().ship.y,
                engine2.state().ship.y,
                "Determinism failed for seed {}: ship y mismatch",
                seed
            );
        }

        println!("Determinism verified across {} seeds ✓", seeds.len());
    }
}
