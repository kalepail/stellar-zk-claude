# Test Fixtures Status

Last reviewed: 2026-02-11

## Canonical Tape Fixtures

- `test-short.tape`
  - Seed: `0xdeadbeef`
  - Frames: `500`
  - Score: `0`
  - Purpose: zero-score rejection path
- `test-medium.tape`
  - Seed: `0xdeadbeef`
  - Frames: `3980`
  - Score: `90`
- `test-real-game.tape`
  - Seed: `0x43c9c6cd`
  - Frames: `13829`
  - Score: `32860`
  - SHA-256: `60f7d595dcf7ebc288723ffb2cf115668d1a95bbaa85530eec62cea36fe67775`

## Additional Validated Tape (Not Canonical)

- `test-real-game-26360.tape`
  - Source: `/Users/kalepail/Downloads/asteroids-19c4dbd5fb7-26360.tape`
  - Seed: `0x4dbd5fb7`
  - Frames: `13001`
  - Score: `26360`
  - SHA-256: `9126d02488bfad307aa2e0caf9537d998df99d8d0868a71387d0e44d4998ee5e`
  - Replay verification: `bun run verify-tape test-fixtures/test-real-game-26360.tape`

## Canonical Groth16 Proof Fixtures

- `proof-medium-groth16.*` and `proof-real-game-groth16.*` are current:
  - Rules digest: `0x41535433` (`AST3`)
  - Image ID: `30cb1dceaa1c626ed7eb906e1567347806eefed4b759b829a3ca3696573b7090`

## Legacy Fixture Note

- `proof-short-groth16.*` is intentionally legacy and only used for zero-score rejection tests.
  - `proof-short-groth16.journal_raw` ends with rules digest `0x41535432` (`AST2`).
  - `proof-short-groth16.image_id` is `2a9fcb04fa9b796a14062bb48c7ddda479c4382cd14ff3053a57f2c327051b30`.
  - This does not affect rejection tests because `submit_score` rejects score `0` before router proof verification.

## Candidate Tape Promotion Guidance

If you consider replacing `test-real-game.tape`, treat it as a breaking fixture change:

1. Verify tape determinism:
   - `bun run verify-tape <candidate.tape>`
2. Replace `test-real-game.tape`.
3. Regenerate non-zero Groth16 fixtures:
   - `bash stellar-asteroids-contract/scripts/regenerate-proofs.sh https://risc0-kalien.stellar.buzz`
4. Update score expectations in tests and scripts (for example `32860` references).
5. Re-run contract and gateway test suites.

Recommendation: keep `test-real-game.tape` as canonical (it already has matching proof fixtures and test expectations) and treat downloaded tapes as additional regression fixtures unless you explicitly want to re-baseline scores and regenerate proofs.
