# Kimi: ZK Verification Strategy & Implementation Guide

> **Strategic synthesis of verification requirements, circuit design, and implementation roadmap for Asteroids ZK proofs**
>
> Based on: `verification-rules.md` (frame-by-frame invariants) + `codex-verification-rules-engine.md` (strict verifier spec)

---

## Executive Summary

This document synthesizes the verification rules from both existing specifications into a concrete **implementation strategy** for the ZK circuit. The goal is to create a system where:

1. **Any bot that follows the rules can produce a valid proof** (optimal play is allowed)
2. **Any bot that breaks the rules CANNOT produce a valid proof** (cheating is impossible)
3. **Verification is efficient enough to run on-chain** (succinct proofs)

The core insight: **The tape contains only inputs (4 bits/frame). The ZK circuit re-simulates the entire game and proves that every state transition followed the rules exactly.**

---

## Part 1: The Verification Philosophy

### 1.1 Why This Approach Works

The current `verify-tape.ts` already enforces rules **by construction** because it replays the game through the canonical engine. However, for ZK proofs, we need explicit constraints that can be verified mathematically.

**The key realization:**
- **Layer 1 (Current):** Engine replay → catches 99.9% of cheating
- **Layer 2 (ZK Circuit):** Constraint satisfaction → cryptographic proof of fair play
- **Layer 3 (Future):** Recursive proofs → scale to millions of games

### 1.2 The Threat Model

**What attackers CANNOT do:**
- Modify the verifier code (it's on-chain/open source)
- Forge a proof for invalid gameplay (constraints won't satisfy)
- Skip frames or duplicate inputs (frame count is fixed)
- Use non-deterministic operations (everything is integer arithmetic)

**What attackers CAN do (and we allow):**
- Build optimal AI bots (optimal play is valid play)
- Pre-compute RNG sequences (deterministic from seed)
- Exploit engine bugs (defense in depth via invariant checks)

**What attackers CANNOT do (and we prevent):**
- Teleport (position computed from velocity)
- Rapid fire (cooldown enforced by circuit)
- Infinite bullets (count checked every frame)
- Score manipulation (only collisions award points)
- Invulnerability hacks (timer decrements enforced)

---

## Part 2: The State Commitment Model

### 2.1 What Must Be Committed

**Public Inputs (Verifier sees these):**
```
- seed: u32                    // Game seed from tape header
- initial_state_hash: bytes32  // Hash of frame 0 state
- final_score: u32             // Claimed final score
- final_rng_state: u32         // Claimed final RNG state
- frame_count: u32             // Number of frames played
```

**Private Inputs (Prover knows these):**
```
- inputs: [u8; frame_count]    // 4-bit inputs per frame
- intermediate_states: [State; frame_count]  // State after each frame
```

**The Circuit Proves:**
```
For each frame i from 0 to frame_count-1:
  1. state[i+1] = transition(state[i], inputs[i])  // Correct transition
  2. All invariants hold on state[i+1]              // Rules followed
  3. state[0] matches initial_state_hash           // Correct start
  4. state[frame_count].score == final_score       // Correct end
  5. state[frame_count].rng == final_rng_state     // Correct RNG
```

### 2.2 State Representation for ZK

Each frame, we commit to a compressed state:

```rust
struct State {
    // Frame metadata
    frame_number: u32,
    rng_state: u32,
    
    // Ship (96 bits)
    ship_x: u16,        // Q12.4
    ship_y: u16,        // Q12.4
    ship_vx: i16,       // Q8.8
    ship_vy: i16,       // Q8.8
    ship_angle: u8,     // BAM
    ship_cooldown: u8,
    ship_invulnerable: u16,
    ship_can_control: bool,
    ship_respawn_timer: u16,
    
    // Game state (96 bits)
    score: u32,
    lives: u8,
    wave: u8,
    next_extra_life: u32,
    time_since_kill: u16,
    saucer_spawn_timer: u16,
    
    // Entity lists (hashed)
    bullets_hash: bytes32,       // Merkle root of bullet list
    asteroids_hash: bytes32,     // Merkle root of asteroid list
    saucers_hash: bytes32,       // Merkle root of saucer list
    saucer_bullets_hash: bytes32,// Merkle root of saucer bullet list
}
```

**Total: ~800 bits per frame commitment** (100 bytes)

---

## Part 3: The Constraint System

### 3.1 Constraint Categories

From the rules documents, we have 100+ constraints organized into categories:

| Category | Count | Complexity | Priority |
|----------|-------|------------|----------|
| Input Validation | 4 | Low | Critical |
| Ship Physics | 11 | Medium | Critical |
| Bullet System | 11 | Medium | Critical |
| Asteroid System | 16 | High | Critical |
| Collision Detection | 20+ | Very High | Critical |
| Saucer AI | 21 | High | High |
| Scoring | 9 | Low | Critical |
| Wave Progression | 5 | Low | Medium |
| Life/Death | 5 | Low | Critical |
| RNG Integrity | 6 | Low | Critical |
| Anti-Lurking | 6 | Low | Medium |

### 3.2 Constraint Examples

**Example 1: Ship Rotation (S-1)**
```rust
// Constraint: angle changes by exactly ±3 BAM based on input
fn constrain_rotation(input: Input, old_angle: u8, new_angle: u8) {
    let expected_delta = if input.left && !input.right {
        -3i16
    } else if input.right && !input.left {
        3i16
    } else {
        0i16
    };
    
    let expected_angle = (old_angle as i16 + expected_delta) & 0xFF;
    assert_eq!(new_angle as i16, expected_angle);
}
```

**Example 2: Speed Limit (S-4)**
```rust
// Constraint: vx² + vy² ≤ MAX_SPEED²
fn constrain_speed(vx: i16, vy: i16) {
    let speed_sq = (vx as i32) * (vx as i32) + (vy as i32) * (vy as i32);
    assert!(speed_sq <= 2105401); // 1451²
}
```

**Example 3: Fire Cooldown (S-7, S-8, S-10)**
```rust
// Constraint: cooldown decrements, can't fire when > 0, resets to 10
fn constrain_fire_cooldown(
    input: Input,
    old_cooldown: u8,
    new_cooldown: u8,
    did_fire: bool
) {
    if old_cooldown > 0 {
        // Cooldown decrements
        assert_eq!(new_cooldown, old_cooldown - 1);
        // Cannot fire during cooldown
        assert!(!did_fire);
    } else {
        // At 0, either fire (reset to 10) or stay at 0
        if did_fire {
            assert_eq!(new_cooldown, 10);
        } else {
            assert_eq!(new_cooldown, 0);
        }
    }
}
```

**Example 4: Bullet Limit (S-9)**
```rust
// Constraint: max 4 player bullets
fn constrain_bullet_count(bullets: &[Bullet]) {
    assert!(bullets.len() <= 4);
}
```

**Example 5: RNG State (R-1)**
```rust
// Constraint: Xorshift32 algorithm
fn constrain_rng(old_state: u32, new_state: u32) {
    let mut x = old_state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    let expected = x as u32;
    assert_eq!(new_state, expected);
}
```

### 3.3 Constraint Cost Analysis

| Operation | Constraints | Notes |
|-----------|-------------|-------|
| Addition/Subtraction | 1-2 | Basic arithmetic |
| Multiplication (32-bit) | 3-5 | Using binary decomposition |
| Comparison (range check) | 2-4 | Range proof techniques |
| Bitwise operations | 1-3 | AND, OR, shifts |
| Array access | 10-20 | Merkle proof for state |
| Hash (Poseidon) | ~100 | For state commitments |
| Trig lookup | 5-10 | Precomputed tables |

**Estimated total per frame: 1,000-2,000 constraints**

For a 5-minute game (18,000 frames):
- **Total constraints: 18-36 million**
- **With chunking (60-frame chunks): 60,000-120,000 per chunk**

---

## Part 4: The Circuit Architecture

### 4.1 High-Level Design

```
┌─────────────────────────────────────────────────────────────┐
│                    ZK Circuit (Rust/Circom)                │
│                                                             │
│  ┌──────────────────────────────────────────────────────┐  │
│  │  Frame Loop (18,000 iterations)                       │  │
│  │                                                       │  │
│  │  ┌──────────────┐    ┌──────────────┐               │  │
│  │  │ Load State   │───▶│ Apply Input  │               │  │
│  │  │ from Merkle  │    │ (4 bits)     │               │  │
│  │  └──────────────┘    └──────────────┘               │  │
│  │                              │                       │  │
│  │                              ▼                       │  │
│  │  ┌──────────────────────────────────────────────┐   │  │
│  │  │           Physics Constraints                │   │  │
│  │  │  ┌──────────┐  ┌──────────┐  ┌──────────┐   │   │  │
│  │  │  │  Ship    │  │ Bullets  │  │Asteroids │   │   │  │
│  │  │  │ Rotation │  │ Movement │  │ Movement │   │   │  │
│  │  │  │ Thrust   │  │ Cooldown │  │  Split   │   │   │  │
│  │  │  │  Drag    │  │  Spawn   │  │   Cap    │   │   │  │
│  │  │  │ Position │  │  Limit   │  │          │   │   │  │
│  │  │  └──────────┘  └──────────┘  └──────────┘   │   │  │
│  │  └──────────────────────────────────────────────┘   │  │
│  │                              │                       │  │
│  │                              ▼                       │  │
│  │  ┌──────────────────────────────────────────────┐   │  │
│  │  │         Collision Constraints                │   │  │
│  │  │  ┌──────────┐  ┌──────────┐  ┌──────────┐   │   │  │
│  │  │  │Bullet-   │  │ Ship-    │  │  Ship-   │   │   │  │
│  │  │  │Asteroid  │  │Asteroid  │  │  Saucer  │   │   │  │
│  │  │  │  Score   │  │   Death  │  │   Kill   │   │   │  │
│  │  │  └──────────┘  └──────────┘  └──────────┘   │   │  │
│  │  └──────────────────────────────────────────────┘   │  │
│  │                              │                       │  │
│  │                              ▼                       │  │
│  │  ┌──────────────────────────────────────────────┐   │  │
│  │  │         Game Logic Constraints               │   │  │
│  │  │  ┌──────────┐  ┌──────────┐  ┌──────────┐   │   │  │
│  │  │  │   RNG    │  │  Score   │  │   Wave   │   │   │  │
│  │  │  │  Update  │  │  Award   │  │ Advance  │   │   │  │
│  │  │  │  Saucer  │  │ Extra    │  │          │   │   │  │
│  │  │  │   AI     │  │   Life   │  │          │   │   │  │
│  │  │  └──────────┘  └──────────┘  └──────────┘   │   │  │
│  │  └──────────────────────────────────────────────┘   │  │
│  │                              │                       │  │
│  │                              ▼                       │  │
│  │  ┌──────────────┐    ┌──────────────┐               │  │
│  │  │ Check All    │───▶│ Commit State │               │  │
│  │  │ Invariants   │    │ to Merkle    │               │  │
│  │  └──────────────┘    └──────────────┘               │  │
│  │                                                       │  │
│  └──────────────────────────────────────────────────────┘  │
│                              │                              │
│                              ▼                              │
│                    ┌──────────────────┐                     │
│                    │ Final Verification                    │
│                    │ - Score matches claim                 │
│                    │ - RNG matches claim                   │
│                    │ - All constraints satisfied           │
│                    └──────────────────┘                     │
└─────────────────────────────────────────────────────────────┘
```

### 4.2 Modular Circuit Design

**Module 1: Fixed-Point Arithmetic**
```rust
// Q12.4 and Q8.8 operations
mod fixed_point {
    fn add_q12_4(a: u16, b: u16) -> u16;
    fn mul_q8_8(a: i16, b: i16) -> i32;
    fn wrap_q12_4(val: u16, max: u16) -> u16;
}
```

**Module 2: Trigonometry**
```rust
// BAM (Binary Angle Measurement) operations
mod trig {
    const SIN_TABLE: [i16; 256];  // Precomputed
    const COS_TABLE: [i16; 256];
    
    fn sin_bam(angle: u8) -> i16;
    fn cos_bam(angle: u8) -> i16;
    fn atan2_bam(dy: i16, dx: i16) -> u8;
}
```

**Module 3: RNG**
```rust
mod rng {
    fn xorshift32(state: u32) -> u32;
    fn next_int(state: u32, max: u32) -> (u32, u32); // (new_state, value)
    fn next_range(state: u32, min: u32, max: u32) -> (u32, u32);
}
```

**Module 4: Physics**
```rust
mod physics {
    fn apply_drag(v: i16) -> i16;
    fn clamp_speed(vx: i16, vy: i16) -> (i16, i16);
    fn update_position(x: u16, vx: i16) -> u16;
}
```

**Module 5: Collision**
```rust
mod collision {
    fn distance_sq_q12_4(ax: u16, ay: u16, bx: u16, by: u16) -> u32;
    fn check_collision(a: Entity, b: Entity) -> bool;
}
```

**Module 6: State Management**
```rust
mod state {
    struct State { /* ... */ }
    
    fn transition(old: State, input: u8) -> State;
    fn check_invariants(state: State) -> Result<(), Violation>;
    fn hash_state(state: State) -> bytes32;
}
```

### 4.3 Circuit Pseudocode

```rust
fn asteroids_circuit(
    public_inputs: PublicInputs,
    private_inputs: PrivateInputs,
) -> Result<(), Error> {
    // Initialize
    let mut state = initialize_state(public_inputs.seed);
    let mut rng_state = xorshift32(public_inputs.seed);
    
    // Verify initial state commitment
    assert_eq!(hash_state(&state), public_inputs.initial_state_hash);
    
    // Process each frame
    for frame in 0..public_inputs.frame_count {
        let input = private_inputs.inputs[frame];
        
        // Validate input format
        assert_eq!(input & 0xF0, 0); // Only 4 bits valid
        
        // Extract input flags
        let fire = (input & 0x01) != 0;
        let thrust = (input & 0x02) != 0;
        let left = (input & 0x04) != 0;
        let right = (input & 0x08) != 0;
        
        // Apply physics
        apply_ship_physics(&mut state, left, right, thrust);
        apply_drag(&mut state);
        clamp_speed(&mut state);
        update_ship_position(&mut state);
        
        // Handle firing
        if fire && state.ship_cooldown == 0 && state.bullets.len() < 4 {
            spawn_bullet(&mut state, &mut rng_state);
            state.ship_cooldown = 10;
        }
        
        // Update bullets
        for bullet in &mut state.bullets {
            bullet.life -= 1;
            update_bullet_position(bullet);
        }
        state.bullets.retain(|b| b.life > 0);
        
        // Update asteroids
        for asteroid in &mut state.asteroids {
            update_asteroid_position(asteroid);
            asteroid.angle = (asteroid.angle + asteroid.spin) & 0xFF;
        }
        
        // Update saucers
        update_saucers(&mut state, &mut rng_state);
        
        // Handle collisions
        handle_collisions(&mut state);
        
        // Update game state
        state.ship_cooldown = state.ship_cooldown.saturating_sub(1);
        state.time_since_kill += 1;
        
        // Check wave completion
        if state.asteroids.is_empty() && state.saucers.is_empty() {
            spawn_wave(&mut state, &mut rng_state);
        }
        
        // Update RNG state
        rng_state = xorshift32(rng_state);
        
        // Check all invariants
        check_invariants(&state)?;
        
        // Commit intermediate state (for chunked proofs)
        if frame % CHUNK_SIZE == 0 {
            commit_state(&state, frame);
        }
    }
    
    // Final verification
    assert_eq!(state.score, public_inputs.final_score);
    assert_eq!(rng_state, public_inputs.final_rng_state);
    
    Ok(())
}
```

---

## Part 5: Optimization Strategy

### 5.1 Chunked Verification

For a 5-minute game (18,000 frames), proving all at once is too expensive. Instead:

```
Chunk Size: 60 frames (1 second)
Number of Chunks: 300 (for 5-minute game)
Proof per Chunk: 60,000-120,000 constraints
Verification per Chunk: ~50ms on consumer hardware
```

**Recursive Proof Composition:**
```
Prove Chunk 0 → Proof 0
Prove Chunk 1 → Proof 1
...
Prove Chunk N → Proof N

Aggregate: 
  Verify(Proof 0) ∧ Verify(Proof 1) ∧ ... ∧ Verify(Proof N)
  → Final Proof
```

This reduces on-chain verification to a single proof verification.

### 5.2 State Commitment Optimization

Instead of committing to the full state every frame, use **Merkle trees**:

```
Entity List → Merkle Root (32 bytes)
- Updates only touch affected leaves
- Proofs of inclusion/non-inclusion
- Efficient for sparse updates
```

### 5.3 Collision Detection Optimization

Naive O(n²) collision is too expensive. Instead:

1. **Spatial Hashing**: Divide world into grid cells
2. **Only check collisions within same/adjacent cells**
3. **Max entities per cell limited by physics**

For Asteroids:
- Max asteroids: 27
- Max bullets: 4 (player) + variable (saucer)
- Max saucers: 3
- **Worst case: ~500 collision checks per frame** (manageable)

### 5.4 Lookup Tables

Precompute and commit to lookup tables:

```rust
// Sine/Cosine tables (BAM angles)
const SIN_TABLE: [i16; 256];
const COS_TABLE: [i16; 256];

// Atan2 table (octant-based)
const ATAN_TABLE: [u8; 33];

// Prover provides index, circuit verifies lookup
```

This converts expensive trig operations to simple array lookups.

---

## Part 6: Implementation Roadmap

### Phase 1: Foundation (Weeks 1-2)
**Goal:** Get the basic circuit compiling and passing tests

**Deliverables:**
- [ ] Set up development environment (Rust + arkworks or Circom)
- [ ] Implement Xorshift32 RNG in circuit
- [ ] Implement fixed-point arithmetic primitives
- [ ] Build trig lookup tables
- [ ] Create basic state struct
- [ ] Write test: single frame with no entities

**Success Criteria:**
- Circuit compiles
- Tests pass
- Constraint count < 1,000 for empty frame

### Phase 2: Ship Physics (Weeks 3-4)
**Goal:** Ship can move and turn with proper physics

**Deliverables:**
- [ ] Ship rotation constraints
- [ ] Thrust acceleration
- [ ] Drag application
- [ ] Speed clamping
- [ ] Position wrapping
- [ ] Fire cooldown

**Tests:**
- Turn left: angle decreases by 3
- Turn right: angle increases by 3
- Thrust: velocity increases in facing direction
- Drag: velocity decreases by ~0.8%
- Speed clamp: velocity capped at 1451 Q8.8
- Wrap: position wraps at world boundaries
- Cooldown: cannot fire more often than every 10 frames

### Phase 3: Bullet System (Weeks 5-6)
**Goal:** Bullets spawn, move, and die correctly

**Deliverables:**
- [ ] Bullet spawning on fire input
- [ ] Bullet velocity (base + ship boost)
- [ ] Bullet lifetime countdown
- [ ] Bullet movement and wrapping
- [ ] Max bullet limit (4)

**Tests:**
- Spawn at ship nose
- Velocity includes ship momentum
- Die after 51 frames
- Max 4 bullets
- Can't fire during cooldown

### Phase 4: Asteroids (Weeks 7-8)
**Goal:** Asteroids spawn, move, split correctly

**Deliverables:**
- [ ] Wave spawn logic
- [ ] Asteroid movement
- [ ] Asteroid splitting
- [ ] Velocity inheritance
- [ ] Asteroid cap (27)

**Tests:**
- Wave 1: 4 large asteroids
- Wave 7+: 16 large asteroids
- Large → 2 medium
- Medium → 2 small
- Small → nothing
- Cap enforced (only 1 child when at cap)

### Phase 5: Collisions (Weeks 9-10)
**Goal:** All collision types work correctly

**Deliverables:**
- [ ] Distance-squared collision
- [ ] Bullet-asteroid collisions
- [ ] Ship-asteroid collisions
- [ ] Bullet-saucer collisions
- [ ] Saucer bullet-asteroid collisions

**Tests:**
- Bullet destroys asteroid
- Score awarded correctly
- Ship dies on collision
- Invulnerability prevents death
- Collision fudge factor applied

### Phase 6: Saucers (Weeks 11-12)
**Goal:** Saucer AI and spawning

**Deliverables:**
- [ ] Saucer spawn timing
- [ ] Anti-lurking mechanics
- [ ] Saucer movement (no X wrap)
- [ ] Saucer firing (aimed vs random)
- [ ] Saucer bullet physics

**Tests:**
- Spawn timer decrements
- Fast spawn when lurking
- Small saucer aims at ship with error
- Large saucer fires randomly
- Off-screen death

### Phase 7: Game State (Weeks 13-14)
**Goal:** Score, lives, waves, game over

**Deliverables:**
- [ ] Scoring system
- [ ] Extra lives at 10k
- [ ] Wave advancement
- [ ] Life loss on death
- [ ] Game over at 0 lives

**Tests:**
- Score only from kills
- Extra life every 10,000
- Wave advances when clear
- Lives decrease on death
- Game over when lives = 0

### Phase 8: Integration & Optimization (Weeks 15-16)
**Goal:** Full game proof generation

**Deliverables:**
- [ ] End-to-end proof generation
- [ ] Chunked proof composition
- [ ] Performance optimization
- [ ] Constraint count reduction
- [ ] Test with real tapes

**Tests:**
- Valid tape from autopilot
- Invalid tape detection
- Performance benchmarks
- Gas cost estimation

### Phase 9: Security & Audit (Weeks 17-18)
**Goal:** Production-ready verifier

**Deliverables:**
- [ ] Comprehensive test suite
- [ ] Adversarial testing
- [ ] Code audit
- [ ] Documentation
- [ ] Deployment guide

**Tests:**
- 100+ positive test cases
- 100+ negative test cases
- Fuzz testing
- Edge case coverage

---

## Part 7: Testing Strategy

### 7.1 Test Categories

**Unit Tests:**
- Each constraint function
- Fixed-point arithmetic
- RNG sequence
- Trig tables

**Integration Tests:**
- Full frame transitions
- Collision scenarios
- Wave progression
- Saucer AI

**System Tests:**
- Complete game proofs
- Invalid tape rejection
- Performance benchmarks

### 7.2 Test Cases (Sample)

**Positive Tests (Must Pass):**
```
1. Empty input (no movement) - ship stays still
2. Turn left 256 times - returns to start angle
3. Fire at max rate - 4 bullets, then cooldown
4. Destroy all asteroids wave 1 - wave 2 spawns
5. Die and respawn - invulnerability, reset position
6. Kill saucer - score awarded
7. Play 5-minute game - proof generates successfully
```

**Negative Tests (Must Fail):**
```
1. Speed hack - velocity > max
2. Teleport - position changes without velocity
3. Rapid fire - bullets < 10 frames apart
4. Extra bullet - 5th bullet spawned
5. Free points - score without kill
6. Wrong RNG - state doesn't match xorshift32
7. Skip wave - wave advances with asteroids alive
8. Invulnerability hack - timer doesn't decrement
9. No drag - velocity constant without thrust
10. Piercing bullet - bullet doesn't die on hit
```

### 7.3 Golden Fixtures

Commit known-good tapes with expected:
- Seed
- Frame count
- Final score
- Final RNG state
- Constraint count

Use these for regression testing.

---

## Part 8: Deployment Considerations

### 8.1 On-Chain Verification

**Target Platforms:**
- Ethereum L1: Use Groth16 or PLONK (small proof size)
- L2 (StarkNet): Use Cairo proofs (native)
- L2 (zkSync): Use Boojum (native)

**Gas Costs (Estimated):**
- Proof verification: ~100k-200k gas
- State commitment: ~20k gas
- Total per submission: ~150k gas

At 20 gwei: ~$6 per verification

### 8.2 Prover Infrastructure

**Options:**
1. **Client-side**: User generates proof in browser (slow but trustless)
2. **Prover network**: Decentralized network of provers (fast, requires trust)
3. **Hybrid**: Client generates small chunks, network aggregates

**Recommendation:** Start with option 1, move to 3 as scale increases.

### 8.3 Economic Model

**Submission Fee:**
- Covers verification gas + prover reward
- Refunded if proof is valid
- Slashed if proof is invalid

**Reward Distribution:**
- Valid high scores earn tokens
- Invalid submissions lose stake
- Provers earn fees for generating proofs

---

## Part 9: Risk Assessment

### 9.1 Technical Risks

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Circuit bug allows exploit | Medium | Critical | Multiple auditors, formal verification, bug bounty |
| Performance too slow | Medium | High | Optimization, chunking, hardware acceleration |
| Constraint explosion | Low | High | Careful design, benchmarking |
| RNG non-determinism | Low | Critical | Only integer ops, no floats |
| State commitment collisions | Very Low | Critical | Use secure hash (Poseidon, Pedersen) |

### 9.2 Security Risks

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Engine-circuit mismatch | Medium | Critical | Extensive testing, cross-validation |
| Malformed tape exploit | Low | High | Strict input validation |
| Replay attack | Very Low | Medium | Include unique submission ID |
| Front-running | Low | Medium | Commit-reveal scheme |

---

## Part 10: Success Criteria

### 10.1 Technical Success

- [ ] Circuit proves valid gameplay correctly
- [ ] Circuit rejects all known cheat vectors
- [ ] Proof generation < 5 minutes on consumer hardware
- [ ] Verification < 200k gas on Ethereum
- [ ] 100% test coverage on critical paths
- [ ] Formal verification of core invariants

### 10.2 Operational Success

- [ ] Valid autopilot tapes prove successfully
- [ ] Invalid tapes are rejected with clear error codes
- [ ] System handles 1000+ submissions per day
- [ ] Prover network is profitable and decentralized

### 10.3 User Success

- [ ] Players trust the high score board
- [ ] Bot developers compete on strategy, not exploits
- [ ] Community grows around fair competition

---

## Appendix A: Quick Reference

### Constants Table

| Constant | Value | Format | Description |
|----------|-------|--------|-------------|
| WORLD_WIDTH_Q12_4 | 15360 | Q12.4 | World width |
| WORLD_HEIGHT_Q12_4 | 11520 | Q12.4 | World height |
| SHIP_TURN_SPEED_BAM | 3 | BAM/frame | Rotation |
| SHIP_THRUST_Q8_8 | 20 | Q8.8 | Acceleration |
| SHIP_MAX_SPEED_Q8_8 | 1451 | Q8.8 | Max velocity |
| SHIP_MAX_SPEED_SQ | 2105401 | Q16.16 | Max speed² |
| SHIP_BULLET_LIMIT | 4 | count | Max bullets |
| SHIP_BULLET_COOLDOWN | 10 | frames | Fire cooldown |
| SHIP_BULLET_LIFETIME | 51 | frames | Bullet life |
| ASTEROID_CAP | 27 | count | Max asteroids |
| LURK_THRESHOLD | 360 | frames | 6 seconds |

### Rule ID Quick Lookup

- **I-***: Initialization rules
- **V-***: Input validation
- **S-***: Ship physics
- **D-***: Ship death/respawn
- **B-***: Bullet rules
- **A-***: Asteroid rules
- **U-***: Saucer rules
- **C-***: Collision rules
- **P-***: Scoring rules
- **W-***: Wave rules
- **L-***: Life/death rules
- **R-***: RNG rules
- **K-***: Anti-lurking rules

---

## Appendix B: Dependencies and Tools

### Recommended Stack

**Circuit Language:**
- **Circom**: Mature, large community, good tooling
- **Noir**: Easier syntax, better IDE support
- **Rust + arkworks**: Most flexible, steeper learning curve

**Recommendation:** Start with Noir for rapid prototyping, migrate to Circom for production.

**Proof System:**
- **Groth16**: Small proofs, trusted setup
- **PLONK**: Universal setup, larger proofs
- **STARKs**: No setup, larger proofs, quantum resistant

**Recommendation:** Groth16 for initial deployment, STARKs for future-proofing.

### Development Tools

- **circomspect**: Static analysis for Circom
- **snarkjs**: Groth16 proving/verification
- **hardhat-circom**: Hardhat integration
- **foundry**: Ethereum testing
- **criterion.rs**: Rust benchmarking

---

## Conclusion

This implementation guide provides a complete roadmap for building the ZK verification system for Asteroids. The key principles are:

1. **Inputs-only tapes** - Prover only submits button presses
2. **Deterministic replay** - Circuit re-simulates every frame
3. **Explicit constraints** - Every rule is a mathematical constraint
4. **Chunked proving** - Scale to long games via composition
5. **Defense in depth** - Multiple layers of verification

With this system, we can guarantee that high scores on the leaderboard were earned fairly, without trusting any central authority. The math proves the game was played by the rules.

**Next Steps:**
1. Set up development environment
2. Implement Phase 1 (Foundation)
3. Begin iterative development with tests
4. Regular security audits
5. Community beta testing

The future of competitive gaming is trustless. Let's build it.
