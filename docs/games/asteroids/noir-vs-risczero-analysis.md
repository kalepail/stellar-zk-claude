# Noir vs RISC Zero: ZK Tape Verification for Asteroids

Deep analysis of two proving systems for verifying deterministic game replay
tapes on Stellar. The tape format records seed + per-frame inputs (~18KB for a
5-minute game at 60fps = ~18,000 frames). Verification means replaying the
entire game inside a ZK circuit and proving the final score matches.

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [The Problem](#2-the-problem)
3. [Noir (UltraHonk)](#3-noir-ultrahonk)
4. [RISC Zero (zkVM)](#4-risc-zero-zkvm)
5. [Pseudo Code: Noir Circuit](#5-pseudo-code-noir-circuit)
6. [Pseudo Code: RISC Zero Guest](#6-pseudo-code-risc-zero-guest)
7. [Comparative Analysis](#7-comparative-analysis)
8. [Stellar On-Chain Verification](#8-stellar-on-chain-verification)
9. [Cost Analysis](#9-cost-analysis)
10. [Recommendation](#10-recommendation)
11. [Implementation Roadmap](#11-implementation-roadmap)
12. [Broader ZK Gaming Ecosystem](#12-broader-zk-gaming-ecosystem)
13. [Key Lessons from ZK Gaming](#13-key-lessons-from-zk-gaming)
14. [Research Provenance](#14-research-provenance)
15. [Sources](#15-sources)

---

## 1. Executive Summary

| Dimension | Noir (UltraHonk) | RISC Zero (zkVM) |
|---|---|---|
| **Language** | Noir DSL | Rust (`no_std`) |
| **Approach** | Custom circuit, chunked recursive proofs | General-purpose zkVM, write Rust |
| **Feasibility** | Possible but complex (must chunk 18K frames) | Straightforward (just port game to Rust) |
| **Monolithic circuit?** | No -- exceeds 2^23 max SRS | Yes -- handles 100M+ cycles |
| **Browser proving** | Yes (2^19 gate ceiling per chunk) | No (server/GPU only) |
| **Native proving time** | ~9 min sequential, ~1-2 min parallel | ~19-23 min GPU, ~2-3 min w/ Boundless |
| **On-chain verifier** | yugocabrio UltraHonk (Soroban) | NethermindEth Groth16 (Soroban) |
| **Proof size** | ~2-4 KB (UltraHonk) | ~256 bytes (Groth16) |
| **Verification cost** | TBD (near 128KiB Soroban limit) | ~$0.003 (~40M CPU instructions) |
| **Proving cost** | Self-hosted only | ~$0.004 via Boundless marketplace |
| **Dev effort** | 10-14 weeks | 6-9 weeks |
| **Main risk** | Circuit complexity, gate budget mgmt | TS-to-Rust determinism parity |

**Bottom line:** RISC Zero is the pragmatic choice -- write Rust, prove with
Boundless for <1 cent, verify on Soroban for <1 cent. Noir is viable if
browser-side proving is a hard requirement, but requires significantly more
engineering effort and circuit optimization expertise.

---

## 2. The Problem

Our Asteroids game uses deterministic integer-only math:
- **Positions:** Q12.4 fixed-point (range 0-4095, 1/16 px precision)
- **Velocities:** Q8.8 fixed-point (range +/-127, 1/256 px/frame precision)
- **Angles:** 8-bit BAM (256 steps per rotation)
- **Trig tables:** 256-entry Q0.14 lookup (values 0-16384)
- **RNG:** Xorshift32 (`x^=x<<13; x^=x>>>17; x^=x<<5`)
- **Timers:** Plain frame counts

A tape records:
```
HEADER (16 bytes): magic, version, seed, frameCount
BODY (N bytes):    one byte per frame (4 input booleans: left, right, thrust, fire)
FOOTER (12 bytes): finalScore, finalRngState, CRC-32
```

**Verification = replay all N frames from seed + inputs, compare final score +
RNG state against footer claims.** Any single divergent frame cascades through
RNG state, making the final RNG state a strong integrity check.

---

## 3. Noir (UltraHonk)

### 3.1 What is Noir?

Noir is Aztec's domain-specific language for writing ZK circuits. It compiles to
an intermediate representation (ACIR) which the Barretenberg backend proves
using the UltraHonk protocol (a PLONKish arithmetization with lookup tables).

### 3.2 Circuit Size Limits

| Constraint Count | Gates | Practical? |
|---|---|---|
| 2^16 | 65,536 | Fast, ~1s |
| 2^19 | 524,288 | **Browser WASM ceiling**, ~10s |
| 2^20 | 1,048,576 | Native only, ~20s |
| 2^21 | 2,097,152 | Native, ~40s |
| 2^22 | 4,194,304 | Native, ~77s |
| 2^23 | 8,388,608 | **Absolute max SRS** -- OOM on 32GB |

### 3.3 Per-Frame Gate Cost Estimate

| Operation | Gates | Notes |
|---|---|---|
| Xorshift32 RNG call | ~30-60 | 3 XOR + 3 shifts; shifts cost ~10 gates each but left-shifts can be replaced with multiplies (1 gate) |
| Trig table lookup | ~13 | ~3.25 gates (dynamic index) + ~10 gates (index mask) |
| Position update (Q12.4 add + wrap) | ~5-10 | Addition is 1 gate; wrapping needs comparison (~10 gates) |
| Velocity update (Q8.8 + shift) | ~15 | Multiply is 1 gate; right-shift >>14 is ~10 gates |
| Drag (`v - (v >> 7)`) | ~11 | Right-shift (10) + subtraction (1) |
| Collision (distance-squared per pair) | ~8-10 | 2 subtracts + 2 multiplies + 1 add + 1 compare |
| Conditionals | full cost | All branches generate gates regardless of execution |

**Conservative estimate: 500-2,000 gates per frame** depending on entity count,
collisions, spawning.

**For 18,000 frames at ~1,000 avg: ~18M gates = 2^24.1**

This **far exceeds the 2^23 max SRS**. A monolithic circuit is impossible.

### 3.4 Solution: Chunked Recursive Proofs

Noir has first-class recursive proof support via `verify_proof`:

```noir
use std::verify_proof;

fn main(
    verification_key: [Field; VK_SIZE],
    proof: [Field; PROOF_SIZE],
    public_inputs: [Field; PI_SIZE],
    key_hash: Field,
    // ... chunk-specific inputs ...
) {
    // Verify the previous chunk's proof
    verify_proof(verification_key, proof, public_inputs, key_hash);
    // ... process this chunk's frames ...
}
```

**Architecture:**
```
Tape (18,000 frames)
  |
  Split into 18-36 chunks of 500-1000 frames
  |
  Chunk 0: seed + frames[0..999] -> state_1, proof_0
  Chunk 1: state_1 + frames[1000..1999] + proof_0 -> state_2, proof_1
  ...
  Chunk 17: state_17 + frames[17000..17999] + proof_16 -> final_state, proof_17
  |
  proof_17 is the final proof submitted on-chain
```

Each chunk: ~1,000 frames * ~1,000 gates = ~1M gates = **2^20 constraints**.
Provable in ~20-40s native, ~2-3 min browser WASM.

### 3.5 Proving Times

| Scenario | Per Chunk | Total (18 chunks) |
|---|---|---|
| Native (M1 Max), sequential | ~20-40s | ~6-12 min |
| Native, parallel (18 cores) | ~30s | ~30s + aggregation |
| Browser WASM, sequential | ~2-3 min | ~36-54 min |
| Browser WASM, parallel (Web Workers) | ~2-3 min | ~3-5 min |

### 3.6 Data Types

Noir supports `u8`, `u16`, `u32`, `u64`, `u128`, `i8`-`i128`, `bool`, `Field`,
fixed arrays `[T; N]`, tuples, and structs. No floating point. Our Q12.4 and
Q8.8 formats map directly to `u32`/`i32`.

**Critical optimization:** Bitwise ops cost ~10 gates each. Left shifts can be
replaced with multiplies for 1 gate:
- `x << 13` becomes `x * 8192` (saves 9 gates)
- `x << 5` becomes `x * 32` (saves 9 gates)
- Right shifts require unconstrained computation + constrained verification

### 3.7 Loops

`for` loops require compile-time-known bounds (fully unrolled). No `while` or
`break` in constrained code. Our `clampSpeedQ8_8()` while-loop must become a
bounded `for` with a max iteration count (3-4 iterations suffice given velocity
ranges).

### 3.8 Lookup Tables

256-entry trig tables with dynamic (witness-dependent) indexing cost ~3.25 gates
per access via Plookup. This is efficient -- polynomial sine approximation in a
circuit would cost more gates.

### 3.9 Noir Game Precedents

- **BattleZips** (Noir): Battleship game, board + shot validation circuits,
  browser-playable, ETHDenver 2023
- **Tikan** (Noir): ZK fog-of-war chess, distance-squared collision (similar to
  ours), separate circuits per operation
- **Dark Forest** (Noir port): DFArchon team proposed 10-week migration from
  Circom. Demonstrates Noir can handle procedural generation.
- **Terry Escape** (Noir): Multiplayer faction warfare with oblivious transfer

### 3.10 Recent Developments

- Noir 1.0 full release expected Q2 2026 (pre-release available)
- Aztec Ignition Chain: 75,000 blocks, zero downtime, 185+ operators
- Goblin Plonk: reduces recursion overhead from ~300K-400K to ~7,296 constraints
- Mopro SDK: native mobile proving (iPhone 16 Pro: 2.63s vs browser 37s)
- WASM Memory64 (phase 5) may eventually raise browser ceiling above 2^19

---

## 4. RISC Zero (zkVM)

### 4.1 What is RISC Zero?

A general-purpose zkVM that executes arbitrary `no_std` Rust compiled to
RV32IM (RISC-V 32-bit with multiply/divide extension). The prover generates a
STARK proof of correct execution, then compresses it to a Groth16 SNARK (~256
bytes) for on-chain verification.

### 4.2 Instruction Costs

| Operation | Cycles |
|---|---|
| Add, compare, jump, shift-left, load, store | 1 |
| AND, OR, XOR, division, remainder, shift-right | 2 |
| Floating-point (emulated) | 60-140 |
| First page access (1KB page, Merkle proof) | 1,094-5,130 |
| SHA-256 per 64-byte block (accelerated) | 68 |

Our integer-only math maps perfectly: additions, comparisons, and left-shifts
cost 1 cycle each. Xorshift32 costs ~8 cycles per call. Float emulation at
60-140 cycles is avoided entirely by our architecture.

### 4.3 Per-Frame Cycle Cost Estimate

| Operation | Cycles | Notes |
|---|---|---|
| Xorshift32 RNG (3-5 calls) | 40-120 | ~8 cycles per call |
| Ship physics (turn, thrust, drag, clamp, wrap) | 60-80 | ~50 integer ops |
| Bullet updates (up to 8 total) | 80-160 | ~20 ops each |
| Asteroid updates (up to 27) | ~270 | ~10 ops each |
| Collision detection (~135 pairs max) | ~2,000 | ~15 ops per pair |
| Saucer AI | ~100 | Spawn timer, movement, aiming |
| Trig lookups | ~10 | 1 cycle each (paged in) |
| Input decoding, scoring, wave mgmt | ~25 | |
| Function call overhead | ~500 | Conservative |
| **Total per frame** | **~3,000-3,500** | |
| **With generous overhead** | **~5,000-8,000** | |

### 4.4 Total Cycle Count

**18,000 frames * ~5,000-8,000 cycles = ~90M-144M cycles**

Cross-reference with known benchmarks:
- Chess verification: ~256K cycles
- Where's Waldo image search: ~8.2M cycles
- DOOM (3,186 ticks, logic only): ~1.7M cycles per tick avg

Our estimate of 50M-150M cycles is conservative and realistic.

### 4.5 Proving Times

| Hardware | Throughput | Time for 100M cycles |
|---|---|---|
| CPU (c6i.8xlarge) | ~15-17 KHz | ~100 min |
| RTX 4090 | ~72-87 KHz | ~19-23 min |
| H100 | ~150-200 KHz | ~8-12 min |
| 10x GPU parallel | ~800 KHz | ~2-3 min |
| Boundless network | distributed | ~1-2 min |

**R0VM 2.0 context:** Achieved 47x speedup for Ethereum block proving (35 min
to 44 sec). Ethereum blocks involve ~30M cycles. Our 100M-cycle game proof is
~3x the work, suggesting ~2-3 min on optimized infrastructure.

### 4.6 Memory

R0VM 2.0 provides 3 GB of user address space. Our game state is ~21 KB total:

| Data | Size |
|---|---|
| Ship state | ~50 bytes |
| Asteroids (27 max) | ~810 bytes |
| Bullets (4 max) | ~100 bytes |
| Saucer + saucer bullets | ~150 bytes |
| Trig tables (sin + cos) | ~1,024 bytes |
| Game state (score, lives, wave, timers) | ~100 bytes |
| Input tape | ~18,000 bytes |
| **Total** | **~21 KB** |

Memory is not a concern.

### 4.7 Boundless Proving Marketplace

Launched September 2025 on mainnet. A decentralized marketplace where GPU
provers compete via reverse Dutch auction to fulfill proof requests.

- **Median cost: $0.04 per billion cycles**
- For 100M cycles: **~$0.004** (<0.5 cents)
- 2,500+ active provers, ~400 trillion cycles/day, 99.9% uptime
- Integration via Rust SDK, REST API, or CLI

### 4.8 STARK-to-Groth16 Compression

1. RISC-V prover generates STARK proof (~200 KB)
2. Recursion layer aggregates segments into one compressed STARK
3. Field transformation converts to BN254 scalar field
4. Groth16 wrapping produces final SNARK

**Final proof: ~256 bytes** (2 G1 points + 1 G2 point on BN254). This is ~500x
compression from the raw STARK.

### 4.9 Development Workflow

```bash
# Install
curl -L https://risczero.com/install | bash && rzup install

# New project
cargo risczero new asteroids-verifier --guest-name verify_tape

# Project structure
asteroids-verifier/
  host/src/main.rs       # Orchestrates proving, reads tape
  methods/guest/src/main.rs  # THE GAME ENGINE (Rust port)
  methods/src/lib.rs     # Exports IMAGE_ID and ELF binary
```

**Development modes:**
- **Dev mode** (`RISC0_DEV_MODE=1`): fake proofs, real execution -- for logic testing
- **Execute only**: prints cycle counts without proving -- for cost estimation
- **Local proving**: real proofs, requires 16+ GB RAM
- **Remote proving** (`BONSAI_API_KEY`): offload to Boundless

### 4.10 ZK Game Precedents

- **DOOM in RISC Zero**: Original 1993 DOOM compiled to RISC-V. 3,186 ticks
  proved in 1m44s (logic only) on GPU. 33+ min with rendering. Proves complex
  game engines work in the zkVM.
- **Dark Forest**: Landmark ZK game, Circom/Groth16, client-side proving
- **Dojo/Starknet**: Provable game engine with 29+ games
- **Elympics**: Deterministic replay verification (inputs-only), "Proof of Game"

---

## 5. Pseudo Code: Noir Circuit

### 5.1 Game State Struct

```noir
struct GameState {
    // Ship
    ship_x: u32,        // Q12.4
    ship_y: u32,        // Q12.4
    ship_vx: i32,       // Q8.8
    ship_vy: i32,       // Q8.8
    ship_angle: u8,     // BAM
    ship_alive: bool,
    ship_respawn_timer: u16,
    ship_invuln_timer: u16,
    ship_fire_cooldown: u8,

    // Asteroids (fixed-size array, MAX 27)
    asteroid_count: u8,
    asteroid_x: [u32; 27],       // Q12.4
    asteroid_y: [u32; 27],       // Q12.4
    asteroid_vx: [i32; 27],      // Q8.8
    asteroid_vy: [i32; 27],      // Q8.8
    asteroid_size: [u8; 27],     // 0=none, 1=small, 2=medium, 3=large
    asteroid_radius: [u16; 27],

    // Bullets (MAX 4 ship + 4 saucer = 8)
    bullet_count: u8,
    bullet_x: [u32; 8],
    bullet_y: [u32; 8],
    bullet_vx: [i32; 8],
    bullet_vy: [i32; 8],
    bullet_life: [u8; 8],
    bullet_from_saucer: [bool; 8],

    // Saucer
    saucer_active: bool,
    saucer_x: u32,
    saucer_y: u32,
    saucer_vx: i32,
    saucer_vy: i32,
    saucer_small: bool,
    saucer_spawn_timer: u16,

    // Game state
    score: u32,
    lives: u8,
    wave: u8,
    rng_state: u32,
    frame_count: u32,

    // Anti-lurking
    time_since_last_kill: u16,
}
```

### 5.2 Chunk Circuit

```noir
// Constants
global CHUNK_SIZE: u32 = 1000;
global WORLD_W: u32 = 15360;   // Q12.4
global WORLD_H: u32 = 11520;   // Q12.4

// Precomputed 256-entry trig tables (Q0.14, values 0..16384)
global SIN_TABLE: [i16; 256] = [ /* ... */ ];
global COS_TABLE: [i16; 256] = [ /* ... */ ];

fn xorshift32(state: u32) -> u32 {
    // Use multiplies instead of shifts where possible (1 gate vs 10)
    let x1 = state ^ (state * 8192);       // x ^= x << 13 (multiply = 1 gate)
    let x2 = x1 ^ (x1 >> 17);              // x ^= x >>> 17 (shift = unconstrained + verify)
    let x3 = x2 ^ (x2 * 32);               // x ^= x << 5 (multiply = 1 gate)
    x3
}

fn apply_drag(v: i32) -> i32 {
    // v - (v >> 7) ~= v * 127/128
    v - (v >> 7)    // unconstrained shift + constrained verify
}

fn wrap_q12_4(val: u32, max: u32) -> u32 {
    if val >= max { val - max }
    else { val }
    // Note: both branches pay full gate cost
}

fn distance_sq_q12_4(x1: u32, y1: u32, x2: u32, y2: u32,
                      w: u32, h: u32) -> u32 {
    let dx = shortest_delta(x1, x2, w);
    let dy = shortest_delta(y1, y2, h);
    // Q12.4 * Q12.4 = Q24.8, fits in u32 for small deltas
    (dx * dx + dy * dy) as u32
}

fn main(
    // Previous chunk's proof (recursive verification)
    prev_vk: [Field; VK_SIZE],
    prev_proof: [Field; PROOF_SIZE],
    prev_public: [Field; PI_SIZE],
    prev_key_hash: Field,

    // This chunk's inputs
    chunk_index: pub u32,
    prev_state_hash: pub Field,
    inputs: [u8; CHUNK_SIZE],       // one byte per frame

    // Previous state (private witness, verified by hash)
    prev_state: GameState,
) -> pub Field {  // returns new state hash

    // 1. Verify previous chunk's proof (skip for chunk 0)
    if chunk_index > 0 {
        verify_proof(prev_vk, prev_proof, prev_public, prev_key_hash);
    }

    // 2. Verify prev_state matches claimed hash
    let computed_hash = poseidon_hash(prev_state);
    assert(computed_hash == prev_state_hash);

    // 3. Simulate CHUNK_SIZE frames
    let mut state = prev_state;

    for frame_idx in 0..CHUNK_SIZE {
        let input = inputs[frame_idx];
        let turn_left  = (input & 0x01) != 0;
        let turn_right = (input & 0x02) != 0;
        let thrust     = (input & 0x04) != 0;
        let fire       = (input & 0x08) != 0;

        // -- Ship update --
        if state.ship_alive {
            // Turning
            if turn_left  { state.ship_angle = state.ship_angle - 3; } // wraps naturally in u8
            if turn_right { state.ship_angle = state.ship_angle + 3; }

            // Thrust
            if thrust {
                let cos_v = COS_TABLE[state.ship_angle as u32] as i32;  // Q0.14
                let sin_v = SIN_TABLE[state.ship_angle as u32] as i32;
                // thrust_q8_8 * cos_q0_14 >> 14 = Q8.8
                state.ship_vx = state.ship_vx + ((20 * cos_v) >> 14);
                state.ship_vy = state.ship_vy + ((20 * sin_v) >> 14);
            }

            // Drag
            state.ship_vx = apply_drag(state.ship_vx);
            state.ship_vy = apply_drag(state.ship_vy);

            // Speed clamp (bounded loop, max 4 iterations)
            for _clamp in 0..4 {
                let speed_sq = state.ship_vx * state.ship_vx
                             + state.ship_vy * state.ship_vy;
                if speed_sq > 2106601 {  // SHIP_MAX_SPEED_SQ_Q16_16
                    state.ship_vx = (state.ship_vx * 3) >> 2;
                    state.ship_vy = (state.ship_vy * 3) >> 2;
                }
            }

            // Position update
            state.ship_x = wrap_q12_4(
                (state.ship_x as i32 + (state.ship_vx >> 4)) as u32,
                WORLD_W
            );
            state.ship_y = wrap_q12_4(
                (state.ship_y as i32 + (state.ship_vy >> 4)) as u32,
                WORLD_H
            );
        }

        // -- Bullet updates, asteroid updates, collision detection --
        // (similar integer arithmetic patterns)
        // ... omitted for brevity but follows same structure ...

        // -- RNG calls for spawning, saucer AI, etc --
        // Each call: state.rng_state = xorshift32(state.rng_state);

        state.frame_count += 1;
    }

    // 4. Return hash of new state
    poseidon_hash(state)
}
```

### 5.3 Final Verification Circuit

```noir
fn verify_game(
    final_vk: [Field; VK_SIZE],
    final_proof: [Field; PROOF_SIZE],
    final_public: [Field; PI_SIZE],
    final_key_hash: Field,

    // Public claims
    seed: pub u32,
    total_frames: pub u32,
    claimed_score: pub u32,
    claimed_rng_state: pub u32,

    // Final state (private, verified by hash from final chunk proof)
    final_state: GameState,
) {
    verify_proof(final_vk, final_proof, final_public, final_key_hash);

    // Verify claims match final state
    assert(final_state.score == claimed_score);
    assert(final_state.rng_state == claimed_rng_state);
    assert(final_state.frame_count == total_frames);
}
```

---

## 6. Pseudo Code: RISC Zero Guest

### 6.1 Guest Program (Rust)

```rust
#![no_std]
#![no_main]

extern crate alloc;
use alloc::vec::Vec;
use risc0_zkvm::guest::env;

// ---- Fixed-point types (matching TypeScript exactly) ----

const WORLD_W_Q12_4: u32 = 15360;
const WORLD_H_Q12_4: u32 = 11520;
const SHIP_THRUST_Q8_8: i32 = 20;
const SHIP_TURN_SPEED_BAM: u8 = 3;
const SHIP_MAX_SPEED_SQ_Q16_16: i32 = 2_106_601;
const SHIP_BULLET_SPEED_Q8_8: i32 = 2219;
const ASTEROID_CAP: usize = 27;

// 256-entry Q0.14 trig tables
static SIN_TABLE: [i16; 256] = [ /* ... exact copy from TypeScript ... */ ];
static COS_TABLE: [i16; 256] = [ /* ... exact copy from TypeScript ... */ ];

fn sin_bam(angle: u8) -> i16 { SIN_TABLE[angle as usize] }
fn cos_bam(angle: u8) -> i16 { COS_TABLE[angle as usize] }

// ---- Xorshift32 RNG (must match TypeScript bit-for-bit) ----

struct Rng { state: u32 }

impl Rng {
    fn new(seed: u32) -> Self {
        Rng { state: if seed == 0 { 1 } else { seed } }
    }

    fn next(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        x
    }

    fn next_range(&mut self, min: i32, max: i32) -> i32 {
        let range = (max - min + 1) as u32;
        min + (self.next() % range) as i32
    }
}

// ---- Game entities ----

#[derive(Clone, Copy)]
struct Ship {
    x: u32,       // Q12.4
    y: u32,
    vx: i32,      // Q8.8
    vy: i32,
    angle: u8,    // BAM
    alive: bool,
    respawn_timer: u16,
    invuln_timer: u16,
    fire_cooldown: u8,
}

#[derive(Clone, Copy)]
struct Asteroid {
    x: u32, y: u32,
    vx: i32, vy: i32,
    size: u8,     // 0=none, 1=small, 2=medium, 3=large
    radius: u16,
}

#[derive(Clone, Copy)]
struct Bullet {
    x: u32, y: u32,
    vx: i32, vy: i32,
    life: u8,
    from_saucer: bool,
}

// ---- Core game loop ----

struct Game {
    ship: Ship,
    asteroids: [Asteroid; ASTEROID_CAP],
    asteroid_count: u8,
    bullets: [Bullet; 8],
    bullet_count: u8,
    // saucer fields ...
    score: u32,
    lives: u8,
    wave: u8,
    rng: Rng,
    frame_count: u32,
    time_since_last_kill: u16,
}

impl Game {
    fn new(seed: u32) -> Self {
        let mut rng = Rng::new(seed);
        let mut game = Game {
            ship: Ship {
                x: WORLD_W_Q12_4 / 2,
                y: WORLD_H_Q12_4 / 2,
                vx: 0, vy: 0,
                angle: 192,  // SHIP_FACING_UP_BAM
                alive: true,
                respawn_timer: 0,
                invuln_timer: 150,  // SHIP_SPAWN_INVULNERABLE_FRAMES
                fire_cooldown: 0,
            },
            asteroids: [Asteroid { x: 0, y: 0, vx: 0, vy: 0, size: 0, radius: 0 }; ASTEROID_CAP],
            asteroid_count: 0,
            bullets: [Bullet { x: 0, y: 0, vx: 0, vy: 0, life: 0, from_saucer: false }; 8],
            bullet_count: 0,
            score: 0,
            lives: 3,
            wave: 0,
            rng,
            frame_count: 0,
            time_since_last_kill: 0,
        };
        game.spawn_wave();
        game
    }

    fn step(&mut self, left: bool, right: bool, thrust: bool, fire: bool) {
        // Ship turning
        if self.ship.alive {
            if left  { self.ship.angle = self.ship.angle.wrapping_sub(SHIP_TURN_SPEED_BAM); }
            if right { self.ship.angle = self.ship.angle.wrapping_add(SHIP_TURN_SPEED_BAM); }

            if thrust {
                let cos_v = cos_bam(self.ship.angle) as i32;
                let sin_v = sin_bam(self.ship.angle) as i32;
                self.ship.vx += (SHIP_THRUST_Q8_8 * cos_v) >> 14;
                self.ship.vy += (SHIP_THRUST_Q8_8 * sin_v) >> 14;
            }

            // Drag: v = v - (v >> 7)
            self.ship.vx -= self.ship.vx >> 7;
            self.ship.vy -= self.ship.vy >> 7;

            // Speed clamp
            loop {
                let speed_sq = self.ship.vx as i64 * self.ship.vx as i64
                             + self.ship.vy as i64 * self.ship.vy as i64;
                if speed_sq <= SHIP_MAX_SPEED_SQ_Q16_16 as i64 { break; }
                self.ship.vx = (self.ship.vx * 3) >> 2;
                self.ship.vy = (self.ship.vy * 3) >> 2;
            }

            // Position update: pos += vel >> 4 (Q8.8 >> 4 = Q12.4 delta)
            self.ship.x = wrap_q12_4(
                (self.ship.x as i32 + (self.ship.vx >> 4)) as u32,
                WORLD_W_Q12_4
            );
            self.ship.y = wrap_q12_4(
                (self.ship.y as i32 + (self.ship.vy >> 4)) as u32,
                WORLD_H_Q12_4
            );

            // Fire bullet
            if fire && self.ship.fire_cooldown == 0
                    && self.bullet_count < 4 {
                self.spawn_bullet();
                self.ship.fire_cooldown = 6; // SHIP_BULLET_COOLDOWN_FRAMES
            }
        }

        // Update bullets, asteroids, saucer, collisions...
        self.update_bullets();
        self.update_asteroids();
        self.update_collisions();
        self.update_saucer();
        self.update_timers();

        self.frame_count += 1;
    }

    fn spawn_wave(&mut self) { /* ... use self.rng ... */ }
    fn spawn_bullet(&mut self) { /* ... */ }
    fn update_bullets(&mut self) { /* ... */ }
    fn update_asteroids(&mut self) { /* ... */ }
    fn update_collisions(&mut self) { /* ... */ }
    fn update_saucer(&mut self) { /* ... */ }
    fn update_timers(&mut self) { /* ... */ }
}

fn wrap_q12_4(val: u32, max: u32) -> u32 {
    if val >= max { val.wrapping_sub(max) }
    else { val }
}

// ---- Tape format ----

struct TapeHeader { seed: u32, frame_count: u32 }
struct TapeFooter { final_score: u32, final_rng_state: u32, checksum: u32 }

fn decode_input(byte: u8) -> (bool, bool, bool, bool) {
    (
        byte & 0x01 != 0,  // left
        byte & 0x02 != 0,  // right
        byte & 0x04 != 0,  // thrust
        byte & 0x08 != 0,  // fire
    )
}

// ---- Entry point ----

risc0_zkvm::guest::entry!(main);

fn main() {
    // Read tape from host
    let header: TapeHeader = env::read();
    let inputs: Vec<u8> = env::read();
    let footer: TapeFooter = env::read();

    // Verify CRC-32 (optional, tape integrity check)
    // let computed_crc = crc32(&raw_tape_bytes);
    // assert_eq!(computed_crc, footer.checksum);

    // Initialize game with tape's seed
    let mut game = Game::new(header.seed);

    // Replay all frames
    for frame in 0..header.frame_count as usize {
        let (left, right, thrust, fire) = decode_input(inputs[frame]);
        game.step(left, right, thrust, fire);
    }

    // Commit verified results to journal (public output)
    env::commit(&header.seed);
    env::commit(&header.frame_count);
    env::commit(&game.score);
    env::commit(&game.rng.state);
}
```

### 6.2 Host Program (orchestrator)

```rust
use methods::{VERIFY_TAPE_ELF, VERIFY_TAPE_ID};
use risc0_zkvm::{default_prover, ExecutorEnv};
use std::fs;

fn main() {
    let tape_bytes = fs::read("game.tape").unwrap();
    let (header, inputs, footer) = parse_tape(&tape_bytes);

    let env = ExecutorEnv::builder()
        .write(&header).unwrap()
        .write(&inputs).unwrap()
        .write(&footer).unwrap()
        .build().unwrap();

    let receipt = default_prover()
        .prove(env, VERIFY_TAPE_ELF)
        .unwrap()
        .receipt;

    // Decode journal (public outputs)
    let journal = receipt.journal.decode::<(u32, u32, u32, u32)>().unwrap();
    let (seed, frame_count, proven_score, proven_rng) = journal;

    println!("Seed: 0x{:08X}", seed);
    println!("Frames: {}", frame_count);
    println!("Verified score: {}", proven_score);
    println!("Final RNG state: 0x{:08X}", proven_rng);

    // Compare against tape claims
    assert_eq!(proven_score, footer.final_score, "Score mismatch!");
    assert_eq!(proven_rng, footer.final_rng_state, "RNG state mismatch!");

    // Save proof for on-chain submission
    let seal = receipt.inner.groth16().unwrap();
    fs::write("proof.bin", seal.to_bytes()).unwrap();
}
```

---

## 7. Comparative Analysis

### 7.1 Developer Experience

| Aspect | Noir | RISC Zero |
|---|---|---|
| Learning curve | New DSL, circuit thinking required | Standard Rust, familiar tooling |
| Debugging | Limited (constraint failures are opaque) | GDB, pprof, cycle counting |
| Testing | Nargo test runner | Standard cargo test |
| Iteration speed | Compile + execute + prove per change | Dev mode: instant execution |
| Game logic | Must manually flatten to circuit-friendly form | Direct port from TypeScript logic |
| Conditionals | All branches pay full gate cost | Normal branching (1 cycle) |
| Loops | Fixed bounds only, fully unrolled | Arbitrary loops, while, break |
| Data structures | Fixed-size arrays, no heap | Vec, BTreeMap, heap allocation |

### 7.2 Performance

| Metric | Noir | RISC Zero |
|---|---|---|
| Browser proving | Yes (2^19 per chunk) | No |
| Native proving (sequential) | ~6-12 min | ~19-23 min (GPU) |
| Native proving (parallel) | ~30s-2 min | ~2-3 min (Boundless) |
| Proof size | ~2-4 KB (UltraHonk) | ~256 bytes (Groth16) |
| Verification time | Logarithmic in circuit size | Constant (~200K gas equiv) |
| On-chain cost | Higher (larger proof) | Lower (tiny proof) |

### 7.3 Architecture Complexity

| Aspect | Noir | RISC Zero |
|---|---|---|
| Chunking required? | Yes (mandatory, 18-36 chunks) | No (single execution) |
| Recursive proof management | Manual (verify_proof per chunk) | Automatic (segment aggregation) |
| State serialization | Poseidon hash between chunks | None needed |
| Circuit optimization | Critical (gate budget management) | Nice-to-have (cycle counting) |
| Bitwise op overhead | 10x (10 gates vs 1 for multiply) | 2x (2 cycles vs 1 for add) |

### 7.4 Ecosystem Maturity

| Aspect | Noir | RISC Zero |
|---|---|---|
| Stellar verifier | yugocabrio UltraHonk (PoC, near Soroban size limit) | NethermindEth Groth16 (production-ready) |
| Proving marketplace | None (self-hosted only) | Boundless ($0.04/billion cycles) |
| Game precedents | BattleZips, Tikan (small circuits) | DOOM (complex game engine) |
| Formal verification | Audits in progress | 122/123 components formally verified |
| Trusted setup | UltraHonk: transparent (no trusted setup!) | Groth16: Powers of Tau ceremony |

---

## 8. Stellar On-Chain Verification

Both paths are enabled by **Protocol 25 (X-Ray)**, live January 22, 2026,
which added native BN254 host functions to Soroban:

- `bls_bn254_g1_add()` -- point addition
- `bls_bn254_g1_mul()` -- scalar multiplication
- `bls_bn254_pairing_check()` -- pairing verification
- Poseidon / Poseidon2 hash primitives

### 8.1 RISC Zero Path (Groth16)

```
Player -> Tape -> Boundless -> STARK proof -> Groth16 compression -> 256 bytes
  -> NethermindEth Soroban verifier -> pairing_check() -> score on-chain
```

- Verifier: `github.com/NethermindEth/stellar-risc0-verifier`
- Verification cost: ~40M CPU instructions (~$0.003)
- Proof fits easily in Soroban transactions
- **Production-ready**

### 8.2 Noir Path (UltraHonk)

```
Player -> Tape -> Recursive proof chain -> Final UltraHonk proof -> ~2-4 KB
  -> yugocabrio Soroban verifier -> BN254 ops -> score on-chain
```

- Verifier: `github.com/yugocabrio/rs-soroban-ultrahonk`
- 244 commits, Nargo v1.0.0-beta.9, deployed to localnet
- Tornado Cash Classic demo on Stellar
- **Near the 128 KiB Soroban contract size limit** -- risk of exceeding
- No trusted setup advantage (UltraHonk is transparent)

---

## 9. Cost Analysis

### 9.1 Per-Game Costs

| Component | Noir | RISC Zero |
|---|---|---|
| Proof generation | Self-hosted (~$0.01-0.05 amortized) | ~$0.004 (Boundless) |
| On-chain verification | ~$0.005-0.01 (estimate) | ~$0.003 |
| Stellar tx fee | ~$0.001 | ~$0.001 |
| **Total per game** | **~$0.02-0.06** | **~$0.008** |

### 9.2 Infrastructure Costs

| Scenario | Noir | RISC Zero |
|---|---|---|
| 100 games/day | ~$3-6/month (own hardware) | ~$2.40/month (Boundless) |
| 1,000 games/day | ~$30-60/month | ~$24/month |
| 10,000 games/day | ~$300-600/month (needs cluster) | ~$240/month |

### 9.3 Development Costs

| Task | Noir | RISC Zero |
|---|---|---|
| Circuit/program development | 6-8 weeks | 2-3 weeks |
| Chunking & recursion management | 2-3 weeks | 0 (automatic) |
| Gate budget optimization | 2-3 weeks | 0 (not critical) |
| Cross-engine determinism testing | 1-2 weeks | 1-2 weeks |
| Soroban integration | 1 week | 1 week |
| E2E integration | 1-2 weeks | 1-2 weeks |
| **Total** | **~10-14 weeks** | **~6-9 weeks** |

---

## 10. Recommendation

### Primary Path: RISC Zero

RISC Zero is the recommended path for these reasons:

1. **Simpler development.** Write standard Rust, no circuit-specific thinking.
   The game logic ports almost 1:1 from TypeScript. No need to manage gate
   budgets, recursive proof chains, or circuit chunking.

2. **Production-ready infrastructure.** NethermindEth's Soroban Groth16
   verifier is deployed and tested. Boundless provides sub-cent proving with
   2,500+ provers and 99.9% uptime.

3. **Smaller proofs, cheaper verification.** Groth16 proofs are ~256 bytes vs
   ~2-4 KB for UltraHonk. On-chain verification costs ~$0.003.

4. **Proven at scale.** DOOM (a much more complex game) has been proven in the
   zkVM. R0VM 2.0 is formally verified (122/123 components).

5. **No chunking complexity.** The zkVM handles segmentation and aggregation
   internally. You write a straight-line replay loop.

### Secondary Path: Noir (if browser proving is required)

Noir becomes the better choice IF:
- Users must generate proofs in their browser (no server dependency)
- The UltraHonk verifier on Soroban matures and fits within size limits
- You're willing to invest 10-14 weeks in circuit development

The chunked recursive architecture works but requires significant engineering:
- Manual circuit decomposition into 500-1000 frame chunks
- Poseidon state hashing between chunks
- Gate budget monitoring and optimization per chunk
- Recursive proof verification overhead per chunk (~7,296 constraints with
  Goblin Plonk, ~300K-400K without)

### Hybrid Approach (Future)

Long-term, a hybrid could offer the best of both:
1. Browser-side: lightweight Noir proof that the player actually played
   (prove 1 frame of input processing, ~2^16 gates, <1s)
2. Server-side: full RISC Zero proof of the entire tape (complete verification)
3. On-chain: Groth16 verification of the RISC Zero proof

---

## 11. Implementation Roadmap

### Phase 1: RISC Zero Guest Program (Weeks 1-3)

1. `cargo risczero new asteroids-verifier`
2. Port game engine from TypeScript to `no_std` Rust
3. Implement deterministic RNG (xorshift32, must match bit-for-bit)
4. Implement fixed-point math (Q12.4, Q8.8, BAM angles, trig tables)
5. Implement core game loop: ship, asteroids, bullets, saucer, collisions
6. Dev mode testing against TypeScript tape outputs

### Phase 2: Cross-Engine Determinism (Weeks 4-5)

1. Generate tapes from TypeScript (autopilot + manual play)
2. Run same tapes through Rust engine
3. Compare frame-by-frame: score, RNG state, entity positions
4. Fix any divergences (the RNG state comparison catches all of them)
5. Build regression test suite with diverse tape corpus

### Phase 3: Proving Pipeline (Weeks 6-7)

1. Local proving with cycle count profiling
2. Optimize hot paths based on pprof flamegraphs
3. Boundless integration (submit proof requests via SDK)
4. End-to-end: tape in, Groth16 proof out

### Phase 4: Soroban Integration (Weeks 8-9)

1. Deploy NethermindEth Groth16 verifier to testnet
2. Build leaderboard contract (stores verified scores + proof refs)
3. Submit proof + journal to verifier contract
4. Read verified scores from leaderboard

### Phase 5: Browser Integration (Weeks 9-10)

1. Tape download triggers Boundless proof request
2. Poll for proof completion
3. Submit proof to Soroban via Freighter wallet
4. Display verified score on leaderboard UI

---

## 12. Broader ZK Gaming Ecosystem

Supplementary research from parallel investigation of ZK gaming precedents,
alternative proving systems, and emerging approaches.

### 12.1 Alternative Proving Systems

**Succinct SP1:** Write Rust → compile to RISC-V → SP1 generates ZK proof.
70%+ of top 1,000 Rust crates work out-of-the-box. Uses STARK recursion +
PLONK SNARK wrapping for Ethereum verification. SP1 Hypercube proved 99.7%
of L1 Ethereum blocks in <12 seconds on 16 NVIDIA RTX 5090 GPUs (May 2025).
If Ethereum block proving takes <12s, game logic could theoretically achieve
sub-second latencies. No Stellar verifier exists yet.

**zkWasm (Delphinus Lab):** Full WASM VM in ZK-SNARK circuits — existing WASM
apps get ZK proofs without modification. CertiK-certified. ZKWASM-Hub (May
2025) provides one-click GitHub-to-chain deployment. Three game models:
ZKProof-based (Rust → WASM → zkWasm), fault-proof/challenge period, and
execution hash stored on-chain.

**Cairo/Starknet:** Validity rollup using STARKs (no trusted setup,
quantum-resistant). Stwo prover: 940x improvement over Stone (first-gen),
500K+ hashes/sec on commodity quad-core CPU. FibRace experiment (Sept 2025):
6,047 players across 99 countries generated 2,195,488 proofs on mobile
devices, majority in under 5 seconds.

### 12.2 IVC / Folding Schemes

An alternative architectural approach beyond chunked recursion:

**Nova:** IVC protocol with constant-size verification circuit. Each folding
step requires only two group scalar multiplications. The expensive SNARK
verification is deferred until game end. Memory grows sublinearly.

**SuperNova:** Extends Nova to non-uniform computation — different step
functions per frame (physics, collision, input handling as separate circuits).
Prover pays cost proportional only to the specific game logic executed per
frame.

**Practical status:** Theoretical foundations well-established but explicit
game implementations are sparse. For 18,000 frames at 50ms per fold ≈ 15
minutes proving time. GPU acceleration essential.

### 12.3 Dojo Framework (Starknet Gaming)

ECS architecture for Cairo smart contracts by Cartridge ($7.5M Series A, 2024):
- **Sozo:** deployment/migration planner
- **Torii:** automatic indexer (GraphQL + gRPC)
- **Katana:** high-speed gaming sequencer (70K+ transactions in Dope Wars playtest)
- **Controller:** Passkeys + Session Tokens for frictionless onboarding

Key games: Loot Survivor (first complex fully on-chain roguelike), Influence
(space MMO with orbital mechanics), Eternum (grand strategy, 8,000 Realms),
Shoshin (async fighting, players program behavior in Cairo). Projects grew
from 4 to 51 on Starknet; 29 of 47 new gaming projects in 2024 used Dojo.

### 12.4 Frame-by-Frame vs Batch Proving

**Hierarchical Recursive Composition (best practice for game proving):**
1. Group 30 frames into "proving windows" (600 windows for 5-min game)
2. Compose 10 windows into window-groups (60 groups)
3. Compose 10 groups into session proofs (6 proofs)
4. Final composition into single proof (~50-100 KB)
5. All levels can be parallelized

Practical estimates: 5-minute game at 60fps → 1-4 hours sequential on
state-of-the-art GPUs. With distributed proving + aggressive batching → 20-60
minutes. Tournament infrastructure ($100K-$250K GPU cluster) → 1,000 game
proofs in ~5 minutes.

### 12.5 Deterministic Replay Verification Precedents

**Elympics:** Most concrete implementation. Stores only player inputs, replays
are deterministic. "Proof of Game" submitted to blockchain. 7-day default
retention, permanent via download.

**Aligned Layer ZK Arcade:** Over 5,000 proofs verified. Players generate
proofs after gameplay, submit to Aligned for verification, results posted to
on-chain leaderboard. 700 whitelist spots for early access.

**Limitations of replay verification:** Cannot verify truthfulness of initial
state (oracle problem), cannot detect hardware-level cheating, cannot detect
social manipulation (match-fixing). Best as part of broader anti-cheat
ecosystem.

### 12.6 Circuit Size Benchmarks (Cross-System)

| Hash Function | R1CS Constraints | Notes |
|---|---|---|
| SHA-256 | ~25,000 | Bitwise-heavy, ZK-hostile |
| MiMC | ~220 muls | Used by Dark Forest |
| Poseidon | ~240 | ZK-friendly, ~20x faster than SHA-256 in circuits |
| Poseidon2 | ~240 (40% faster proving) | 50% less RAM than MiMC |
| Griffin | ~96 | Fastest for R1CS |

| Prover | Keccak256 Proofs/sec |
|---|---|
| Expander | 16,700 |
| Plonky3 | 1,368 |
| GNARK | 4.5 |
| Halo2 | <1 |

**Key cost inversions (ZK vs CPU):** Bitwise ops (AND/OR/XOR/shifts): 1 CPU
cycle → ~10+ circuit gates. Addition/multiplication: multiple CPU cycles → 1
circuit gate. This directly impacts our game — drag computation via `>> 7`
costs ~10x more in circuits than a field multiply.

---

## 13. Key Lessons from ZK Gaming

Distilled from Dark Forest, MUD, Dojo, and the broader ZK gaming ecosystem:

1. **Under-constrained circuits are the #1 ZK security vulnerability** (~2/3 of all ZK bugs per 0xPARC tracker)
2. **Integer-only arithmetic is mandatory** for deterministic ZK proving — floating-point is non-deterministic across platforms and non-associative
3. **Separate game logic from rendering** — only prove state transitions. RISC Zero DOOM: logic-only proving was 20x faster than with rendering
4. **Choose ZK-friendly primitives** — Poseidon is 20x faster than SHA-256 in circuits. Avoid bitwise operations in proven code. Use lookup tables for trig functions
5. **Frame batching + hierarchical aggregation** is the practical architecture. Frame-by-frame proving is elegant but impractical without dedicated GPU clusters
6. **Client-side proving is becoming real** — FibRace: 6,047 players, 2.2M proofs on mobile phones, <5s each (Stwo prover)
7. **Use unconstrained execution** for expensive operations (division, sorting) — compute outside the circuit, verify the result inside
8. **zkVMs (SP1, RISC Zero) vs custom circuits:** zkVMs are 10-100x slower but let you write normal Rust. Choose based on proving time budget
9. **Lookup tables** dramatically reduce circuit size for restricted-domain operations (trig, damage tables)
10. **Proof size matters for on-chain verification** — Groth16: ~128 bytes, PLONK: ~1 KB, STARK: 50-200 KB
11. **300 Dark Forest players congested an entire testnet** — scaling must be planned from day one
12. **Smart contracts cannot generate ZK proofs** — fundamental architectural constraint
13. **Plugin ecosystems are essential** for maintaining engagement (Dark Forest lesson)
14. **Formal verification of ZK circuits** is essential but tooling is immature (RISC Zero + SP1 pioneering with Veridise/Nethermind)
15. **ZK-rollup production costs dropped 90%** from 2024 to 2025 ($0.05-$0.10 → $0.004 per tx) driven by GPU acceleration and algorithm improvements

---

## 14. Research Provenance

This document was synthesized from three parallel research agents using
Perplexity deep research, web searches, and codebase analysis. All research
was conducted on February 5, 2026.

### Research Agent IDs

| Agent ID | Focus Area | Perplexity Queries | Key Sources |
|---|---|---|---|
| `a8d0e77` | Noir ZK DSL analysis | ~200 | Noir docs, Barretenberg, BattleZips, Tikan, Mopro, yugocabrio |
| `a6b3d48` | RISC Zero zkVM analysis | ~200 | RISC Zero docs, Boundless, NethermindEth, R0VM 2.0 blog |
| `a7ecc58` | ZK game proving precedents | ~186 | Dark Forest, 0xPARC, Dojo, Elympics, Aligned, SP1, zkWasm |

**Total Perplexity research queries:** ~586 across all agents.

**Note:** Perplexity MCP does not produce persistent session/research IDs.
The agent IDs above are the trackable identifiers. Full agent transcripts
including all Perplexity research results are preserved in the session
transcript at the time of writing.

### Session Metadata

- **Claude Code session:** `acfc2ebe-4f59-4fca-872d-3b0b46d28c90`
- **Git branch:** `feature/replay-tape`
- **Model:** Claude Opus 4.6
- **Date:** February 5, 2026

---

## 15. Sources

### Noir / UltraHonk
- [Noir Language Documentation](https://noir-lang.org/docs)
- [Barretenberg Backend](https://github.com/AztecProtocol/barretenberg)
- [BattleZips Noir](https://github.com/BattleZips/BattleZips-Noir)
- [Tikan (ZK Chess)](https://github.com/tsujp/tikan)
- [Dark Forest Noir Port](https://github.com/SleepingShell/darkforest-noir)
- [yugocabrio UltraHonk Verifier for Soroban](https://github.com/yugocabrio/rs-soroban-ultrahonk)
- [Noir GitHub Discussions: Soroban Verifier](https://github.com/orgs/noir-lang/discussions/8509)
- [Mopro Mobile SDK](https://github.com/zkmopro/mopro)

### RISC Zero
- [RISC Zero Documentation](https://dev.risczero.com)
- [RISC Zero DOOM Blog Post](https://risczero.com/blog/when-the-doom-music-kicks-in)
- [R0VM 2.0 Announcement](https://risczero.com/blog/r0vm-2-0)
- [Boundless Documentation](https://docs.boundless.xyz)
- [NethermindEth Stellar RISC Zero Verifier](https://github.com/NethermindEth/stellar-risc0-verifier)
- [RISC Zero Formal Verification](https://risczero.com/blog/RISCZero-formally-verified-zkvm)

### Stellar
- [Protocol 25 X-Ray Announcement](https://stellar.org/blog/developers/announcing-stellar-x-ray-protocol-25)
- [Stellar ZK Overview](https://stellar.org/learn/zero-knowledge-proof)
- [CAP-0074: BN254 Host Functions](https://stellar.org/protocol/cap-0074)
- [CAP-0075: Poseidon Hash](https://stellar.org/protocol/cap-0075)

### ZK Game Precedents
- [Dark Forest Circuits](https://github.com/darkforest-eth/circuits)
- [Dark Forest Init Circuit Technical Writeup](https://blog.zkga.me/df-init-circuit)
- [Dark Forest Naavik Deep Dive](https://naavik.co/deep-dives/dark-forest-beacon-of-light/)
- [ZK Hunt (0xPARC)](https://github.com/FlynnSC/zk-hunt)
- [Dojo Framework](https://dojoengine.org/framework)
- [Cartridge](https://cartridge.gg)
- [Starknet On-Chain Gaming](https://www.starknet.io/blog/onchain-gaming/)
- [Dungeons and Dojos (MUD vs Dojo)](https://www.bitkraft.vc/insights/dungeons-and-dojos-exploring-onchain-game-development-with-mud-and-dojo/)
- [Elympics Verifiable Replays](https://www.elympics.ai/products/verifiable-replays)
- [Elympics Proof of Game](https://www.elympics.ai/solutions/proof-of-game)
- [Aligned ZK Arcade](https://zkarcade.com)
- [Aligned 2025 Recap](https://blog.alignedlayer.com/aligned-2025-recap/)
- [0xPARC ZK Bug Tracker](https://github.com/0xPARC/zk-bug-tracker)

### Alternative Proving Systems
- [SP1 Introduction](https://blog.succinct.xyz/introducing-sp1/)
- [SP1 Real-Time Proving on 16 GPUs](https://blog.succinct.xyz/real-time-proving-16-gpus/)
- [SP1 Proof Aggregation Docs](https://docs.succinct.xyz/docs/sp1/writing-programs/proof-aggregation)
- [Delphinus Lab zkWasm](https://delphinuslab.com/tutorial/zkwasm-its-ecosystems/)
- [Provable Game using ZKWASM-MINI-ROLLUP](https://delphinuslab.com/2024/09/10/provable-game-using-zkwasm-mini-rollup/)
- [Stwo Prover Announcement](https://starkware.co/blog/s-two-prover/)

### IVC / Folding Schemes
- [Nova: Incrementally Verifiable Computation (Lambda Class)](https://blog.lambdaclass.com/incrementally-verifiable-computation-nova/)
- [Folding Schemes Deep Dive (Srinath Setty)](https://hackmd.io/@srinathsetty/folding-schemes)
- [SuperNova / HyperNova Paper](https://www.andrew.cmu.edu/user/bparno/papers/hypernova-1.pdf)
- [Awesome Folding (GitHub)](https://github.com/lurk-lab/awesome-folding)
- [Folding Endgame (zkresear.ch)](https://zkresear.ch/t/folding-endgame/106)

### Benchmarks
- [Proof Arena](https://www.proof-arena.com)
- [zkbenchmarks.com](https://zkbenchmarks.com)
- [Aligned ZK Benchmarks](https://blog.alignedlayer.com/zkbenchmarks/)
- [Celer ZK Framework Benchmarks](https://blog.celer.network/2023/08/04/the-pantheon-of-zero-knowledge-proof-development-frameworks/)
- [Ethereum Foundation zkEVM Benchmarking](https://zkevm.ethereum.foundation/blog/benchmarking-zkvms)
- [ZK Proof Generation Costs 2025](http://blockeden.xyz/forum/t/zk-proof-generation-costs-dropped-90-in-2025-heres-how-hardware-changed-everything/281)
