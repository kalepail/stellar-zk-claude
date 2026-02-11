/**
 * Headless tape verifier.
 *
 * Usage: bun run scripts/verify-tape.ts <tape-file>
 *
 * Reads a .tape file, replays it in headless mode, and compares
 * the final score + RNG state against the tape footer.
 * Exit 0 = PASSED, Exit 1 = FAILED.
 */

import { readFileSync } from "fs";
import { AsteroidsGame } from "../src/game/AsteroidsGame";
import { TapeInputSource } from "../src/game/input-source";
import { deserializeTape } from "../src/game/tape";

const DEFAULT_MAX_FRAMES = 18_000;

const tapePath = process.argv[2];

if (!tapePath) {
  console.error("Usage: bun run scripts/verify-tape.ts <tape-file>");
  process.exit(1);
}

const data = new Uint8Array(readFileSync(tapePath));
const tape = deserializeTape(data, DEFAULT_MAX_FRAMES);

console.log(`Tape: ${tapePath}`);
console.log(`  Seed:       0x${tape.header.seed.toString(16).padStart(8, "0")}`);
console.log(`  Frames:     ${tape.header.frameCount}`);
console.log(`  Exp. Score:  ${tape.footer.finalScore}`);
console.log(`  Exp. RNG:   0x${tape.footer.finalRngState.toString(16).padStart(8, "0")}`);
console.log();

// Create headless game
const game = new AsteroidsGame({ headless: true, seed: tape.header.seed });

// Start game first (resets state), then set tape input source
game.startNewGame(tape.header.seed);
const source = new TapeInputSource(tape.inputs);
game.setInputSource(source);

// Run simulation
const start = performance.now();

for (let i = 0; i < tape.header.frameCount; i++) {
  game.stepSimulation();
}

const elapsed = performance.now() - start;

const actualScore = game.getScore();
const actualRng = game.getRngState();

console.log(`Replay complete in ${elapsed.toFixed(1)}ms (${(tape.header.frameCount / (elapsed / 1000)).toFixed(0)} fps)`);
console.log(`  Score:  ${actualScore} (expected ${tape.footer.finalScore})`);
console.log(`  RNG:    0x${actualRng.toString(16).padStart(8, "0")} (expected 0x${tape.footer.finalRngState.toString(16).padStart(8, "0")})`);

const scoreOk = actualScore === tape.footer.finalScore;
const rngOk = (actualRng >>> 0) === (tape.footer.finalRngState >>> 0);

if (scoreOk && rngOk) {
  console.log("\nVERIFICATION PASSED");
  process.exit(0);
} else {
  if (!scoreOk) console.error(`  Score mismatch: got ${actualScore}, expected ${tape.footer.finalScore}`);
  if (!rngOk) console.error(`  RNG mismatch: got 0x${actualRng.toString(16)}, expected 0x${tape.footer.finalRngState.toString(16)}`);
  console.error("\nVERIFICATION FAILED");
  process.exit(1);
}
