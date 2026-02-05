# Codex Research: Asteroids Tape Verification in Noir vs RISC Zero

Date: 2026-02-05  
Status: Research synthesis + implementation proposals

## 1) Why This Document Exists

We need a proving system for Asteroids tape verification that is:
- strict enough to enforce game rules (no teleports, no illegal fire rate, no fake score/wave),
- scalable to long replays (target up to 18,000 frames),
- viable for eventual on-chain verification on Soroban.

This doc compares **Noir/UltraHonk** vs **RISC Zero** for that exact workload and gives concrete starter plans for either path.

## 2) Inputs Used

### Local project docs and code
- `docs/zk/verification-rules.md`
- `docs/zk/integer-math-reference.md`
- `docs/zk/03-PROVING-SYSTEMS.md`
- `docs/zk/09-GETTING-STARTED.md`
- `src/game/AsteroidsGame.ts`
- `src/game/tape.ts`

### External research (primary sources emphasized)
- Noir docs (v1.0.0-beta.18), recursion docs, NoirJS browser tutorial.
- Noir issue #2543 (browser proving limits and chunking pain).
- Barretenberg docs.
- RISC Zero dev docs (quickstart, recursion, proving options, shrink-wrapping, local/remote proving, verifier contracts).
- RISC Zero docs.rs API (`ReceiptKind`, `prove_with_opts`).
- Stellar docs resource limits.
- Soroban UltraHonk repos and discussion/milestone pages.
- Perplexity deep research pass (used as secondary synthesis, with claims cross-checked against primary docs).

## 3) Hard Constraint: 18,000-Frame Replay Size

From `docs/zk/integer-math-reference.md`, rough estimate is **~2,000 constraints/frame**.

Inference:
- 18,000 frames x 2,000 ~= **36,000,000 constraints** (order of magnitude).

Noir issue #2543 states current browser WASM ceiling is about **2^19 constraints** (~524,288) for UltraPlonk/Barretenberg in desktop browsers, tied to WASM memory constraints.

Inference:
- Full single-proof browser replay is not realistic at 18,000 frames.
- Even chunked proofs need many chunks (dozens to >100, depending on chunk design and overhead).

## 4) Noir Deep Dive (UltraHonk / Barretenberg)

## What is strong
- Developer-friendly circuit DSL with Rust-like syntax.
- NoirJS + bb.js supports browser witness/proof flows.
- Recursion support exists in Noir stdlib (`std::verify_proof`).
- Active Soroban UltraHonk work exists (`rs-soroban-ultrahonk` and related repos).

## What is risky for this use case
- Browser proving for large workloads hits memory/constraint ceilings quickly.
- Recursive chunking workflow for large browser proofs is still largely manual in practice (see issue #2543).
- Noir recursion docs explicitly note recursive proof validity is enforced by backend proof generation, not witness execution itself.
- Soroban UltraHonk path is still maturing:
  - localnet success exists,
  - but milestone notes show `Error(Budget, ExceededLimit)` without unlimited settings and ongoing optimization/precompile work.

## Practical conclusion for Asteroids full replay
- **Noir is viable for smaller/chunked proofs**, but for 18,000-frame full replay fairness proof in-browser, it is currently high-friction and high-risk unless we commit to a chunk/aggregation architecture from day one.

## 5) RISC Zero Deep Dive

## What is strong
- Proves arbitrary Rust program execution directly (natural fit for deterministic simulation replay).
- Explicit support for recursion pipeline and unbounded-size handling through segmented execution:
  - segment receipts -> succinct -> Groth16 wrapping.
- Official options for proof kinds (`Composite`, `Succinct`, `Groth16`) via `prove_with_opts`.
- Remote proving path (Boundless/Bonsai) exists for heavy workloads.
- Clear on-chain verifier contract model in ecosystem docs.

## Important caveats
- Proof generation is not browser-native in the same way NoirJS is.
- Local Groth16 prover has hardware caveat in docs (x86-only note), so for some environments remote proving or alternative host setup may be necessary.

## Practical conclusion for Asteroids full replay
- **RISC Zero is the lowest-risk path for v1 full-replay fairness proofs** because it maps directly to replaying deterministic engine logic and handles long computation shape better.

## 6) Soroban Reality Check

Stellar docs list per-transaction limits (notably CPU/memory/tx size), so verifier contracts must stay inside these budgets.  
This matters for both approaches, but current UltraHonk-on-Soroban notes explicitly show budget pressure in practical tests, while RISC Zero verifier architecture on Stellar is already represented in ecosystem docs and repos.

## 7) Recommendation

## Decision for v1
- Choose **RISC Zero** for canonical full-tape fairness proof.
- Keep Noir as an R&D track, not primary correctness path.

## Why
- We already have deterministic replay logic and rule catalog; RISC Zero can prove that exact computation model with less translation into circuit-specific constraints.
- 18,000-frame target strongly stresses browser/circuit ceilings on Noir.
- RISC Zero recursion/receipt pipeline is explicit and designed for this shape.

## Decision for v2
- Run a **parallel Noir pilot** for chunked browser proofs if product priority is client-side proving UX.
- Only graduate Noir path to production after:
  - chunk/aggregation architecture is stable,
  - Soroban resource-budget verification is validated under realistic limits.

## 8) Starter Architecture: RISC Zero (Recommended v1)

## Public claim
- `(seed, frame_count, final_score, final_rng_state, rules_version_hash, tape_crc)`

## Guest program pseudocode (Rust)
```rust
use risc0_zkvm::guest::env;

fn main() {
    let tape: Tape = env::read(); // seed + inputs + footer

    assert!(tape.header.magic == ZKTP_MAGIC);
    assert!(tape.header.version == 1);
    assert!(crc32(&tape.header_and_body()) == tape.footer.checksum);

    let mut state = GameState::new(tape.header.seed);
    for (frame, input_byte) in tape.inputs.iter().enumerate() {
        assert!((input_byte & 0xF0) == 0); // reserved bits must be zero

        let input = decode_input(*input_byte);
        let prev = state.snapshot();
        state.step(input);
        verify_frame_rules(frame as u32, &prev, &state, input); // from rules-engine spec
    }

    assert!(state.score == tape.footer.final_score);
    assert!(state.rng_state() == tape.footer.final_rng_state);

    env::commit(&PublicOutput {
        seed: tape.header.seed,
        frame_count: tape.header.frame_count,
        final_score: state.score,
        final_rng_state: state.rng_state(),
        rules_version_hash: RULES_VERSION_HASH,
        tape_crc: tape.footer.checksum,
    });
}
```

## Host/prover pseudocode (Rust)
```rust
let env = ExecutorEnv::builder().write(&tape).build()?;
let opts = ProverOpts::groth16(); // or succinct then compress
let prove_info = prover.prove_with_opts(env, ASTEROIDS_ELF, &opts)?;
prove_info.receipt.verify(ASTEROIDS_IMAGE_ID)?;

// submit seal + journal to verifier contract / app backend
```

## Soroban integration pseudocode
```rust
pub fn submit_proof(env: Env, seal: Bytes, journal: Bytes) {
    let out = parse_public_output(&journal);
    assert_eq!(out.rules_version_hash, RULES_VERSION_HASH);

    // call verifier router / verifier contract
    verifier::Client::new(&env, &VERIFIER_ID)
        .verify(&seal, &ASTEROIDS_IMAGE_ID, &sha256(&journal));

    leaderboard::record(env, out.final_score, out.frame_count, out.tape_crc);
}
```

## 9) Starter Architecture: Noir (R&D / v2 Candidate)

## Core design constraint
- Must chunk replay into fixed-size circuits and recursively aggregate.
- Do not attempt single-circuit 18,000-frame browser proof.

## Proposed chunk model
- `CHUNK_FRAMES = 128` (starting point; tune by empirical proving budget).
- `chunk_transition.nr` proves exact transitions + rules for one chunk.
- `agg.nr` recursively verifies chunk proofs and enforces state-chain continuity.

## Chunk circuit pseudocode (Noir)
```noir
global CHUNK_FRAMES: u32 = 128;

fn main(
    start_state_commit: pub Field,
    inputs: [u8; CHUNK_FRAMES],
    end_state_commit: pub Field
) {
    let mut s = open_state(start_state_commit);

    for i in 0..CHUNK_FRAMES {
        assert((inputs[i] & 0xF0) == 0);
        let prev = s;
        s = step(prev, decode_input(inputs[i]));
        assert_rules(prev, s, decode_input(inputs[i]));
    }

    assert(hash_state(s) == end_state_commit);
}
```

## Aggregation circuit pseudocode (Noir recursion)
```noir
fn main(
    vk: [Field; VK_LEN],
    key_hash: Field,
    proofs: [[Field; PROOF_LEN]; N],
    public_inputs: [[Field; PUB_LEN]; N],
    final_commit: pub Field
) {
    for i in 0..N {
        std::verify_proof(vk, proofs[i], public_inputs[i], key_hash);
        if i > 0 {
            assert(public_inputs[i-1][END_COMMIT_IDX] == public_inputs[i][START_COMMIT_IDX]);
        }
    }
    assert(public_inputs[N-1][END_COMMIT_IDX] == final_commit);
}
```

## Browser orchestrator pseudocode
```ts
for (chunk of chunkTape(inputs, CHUNK_FRAMES)) {
  const witness = await noir.execute(chunkInputs, startCommit);
  const proof = await backend.generateProof(witness);
  proofs.push(proof);
  startCommit = extractEndCommit(proof.publicInputs);
}
const aggProof = await proveAggregation(proofs);
```

## 10) Proposed Work Plan (Either/Both)

## Track A (ship first): RISC Zero
1. Port deterministic replay core + rules checks to Rust.
2. Reproduce local `verify-tape` parity on golden tapes.
3. Integrate RISC Zero guest/host and generate Groth16 receipts.
4. Add Soroban verifier submission flow and leaderboard write path.

## Track B (parallel R&D): Noir
1. Build minimal chunk transition circuit for 16-32 frames.
2. Measure browser proof memory/time scaling.
3. Add recursive aggregator PoC.
4. Evaluate Soroban verification budget with realistic chunk count.

## Exit criteria to keep Noir in contention
- Stable chunk proving in browser at acceptable latency.
- Aggregation proves full 18,000-frame runs.
- Soroban verification path fits budget without privileged localnet settings.

## 11) Bottom-Line Answer to “Is Noir out because browser-based?”

For **full 18,000-frame Asteroids fairness proofs**, Noir is **not out forever**, but **it is not the practical v1 path** unless we accept substantial chunking/recursion complexity immediately.  

RISC Zero is the safer path to get strict, production-grade fairness verification online first.

## 12) Sources

### Local repo
- `docs/zk/integer-math-reference.md`
- `docs/zk/verification-rules.md`
- `docs/zk/03-PROVING-SYSTEMS.md`
- `docs/zk/09-GETTING-STARTED.md`

### External primary sources
- Noir docs: https://noir-lang.org/docs/
- Noir browser tutorial: https://noir-lang.org/docs/tutorials/noirjs_app
- Noir recursion docs: https://noir-lang.org/docs/noir/standard_library/recursion
- Noir issue on browser proving/chunking: https://github.com/noir-lang/noir/issues/2543
- Barretenberg docs: https://barretenberg.aztec.network/docs/
- RISC Zero quickstart: https://dev.risczero.com/api/zkvm/quickstart
- RISC Zero recursion: https://dev.risczero.com/api/recursion
- RISC Zero proving options: https://dev.risczero.com/api/generating-proofs/proving-options
- RISC Zero local proving notes: https://dev.risczero.com/api/generating-proofs/local-proving
- RISC Zero remote proving: https://dev.risczero.com/api/generating-proofs/remote-proving
- RISC Zero verifier contracts: https://dev.risczero.com/api/blockchain-integration/contracts/verifier
- RISC Zero shrink-wrapping: https://dev.risczero.com/api/blockchain-integration/shrink-wrapping
- `ReceiptKind` docs.rs: https://docs.rs/risc0-zkvm/3.0/risc0_zkvm/enum.ReceiptKind.html
- Soroban limits/fees: https://developers.stellar.org/docs/networks/resource-limits-fees
- Stellar RISC Zero verifier repo: https://github.com/NethermindEth/stellar-risc0-verifier
- Soroban UltraHonk repo: https://github.com/yugocabrio/rs-soroban-ultrahonk
- Noir discussion #8509: https://github.com/orgs/noir-lang/discussions/8509
- Noir discussion #8560: https://github.com/orgs/noir-lang/discussions/8560
- Milestone writeup: https://hackmd.io/@indextree/rJPW3jU6lx

### Secondary synthesis
- Perplexity deep research run (cross-checked against primary links above).

## 13) Research Run IDs (Traceability)

### Parallel Search MCP (`web_search_preview`) IDs
- `search_ddaf5be936134343914722f3ebfa87fc`
- `search_5f67f36954dc4365b373500470cddca3`
- `search_d26d2c14c7e74dc5be12c9705c43f1ce`
- `search_b1b537ce98434a35b745ec72dab385e2`

### Parallel Search MCP (`web_fetch`) IDs
- `extract_71c815bb5f8c4f0ab5f02fc7f78fd9a4`
- `extract_97b9b18b284c4530aa3e4c666b3e4114`
- `extract_803367d8c09f49a98d8ee39be7c834a7`
- `extract_1d51f81ee83c4e709ac675c41cadc535`
- `extract_1ea36c10e304462d840c9263842552c1`
- `extract_c825109fa1b942b4abe3ca3f64df8886`
- `extract_421be4bdbeeb409cb107416fda31b3df`

### Perplexity MCP runs used
- `perplexity_research`: used for high-level comparison synthesis (claims were cross-checked against primary sources before inclusion).
- `perplexity_reason`: attempted targeted verdict query; result set was irrelevant and not used as a source of truth.

Note: the Perplexity MCP responses in this session did not expose a stable run identifier field analogous to `search_*` / `extract_*`.
