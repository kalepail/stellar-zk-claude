/**
 * Dumps intermediate game state during tape replay for cross-language debugging.
 *
 * Usage: bun run scripts/dump-tape-state.ts <tape-file> [--every <N>]
 *
 * Outputs JSON lines: one per sampled frame with RNG state, score, entity counts, etc.
 */

import { readFileSync } from "fs";
import { AsteroidsGame } from "../src/game/AsteroidsGame";
import { TapeInputSource } from "../src/game/input-source";
import { deserializeTape } from "../src/game/tape";

const tapePath = process.argv[2];
let everyN = 1;

const args = process.argv.slice(3);
for (let i = 0; i < args.length; i++) {
  if (args[i] === "--every" && args[i + 1]) {
    everyN = parseInt(args[++i], 10);
  }
}

if (!tapePath) {
  console.error("Usage: bun run scripts/dump-tape-state.ts <tape-file> [--every <N>]");
  process.exit(1);
}

const data = new Uint8Array(readFileSync(tapePath));
const tape = deserializeTape(data);

console.error(`Tape: ${tapePath}`);
console.error(`  Seed: 0x${tape.header.seed.toString(16).padStart(8, "0")}`);
console.error(`  Frames: ${tape.header.frameCount}`);

const game = new AsteroidsGame({ headless: true, seed: tape.header.seed });
game.startNewGame(tape.header.seed);
const source = new TapeInputSource(tape.inputs);
game.setInputSource(source);

// Dump initial state (frame 0, before any simulation)
console.log(JSON.stringify({
  frame: 0,
  rng: game.getRngState() >>> 0,
  score: game.getScore(),
  lives: game.getLives(),
  wave: game.getWave(),
}));

for (let i = 1; i <= tape.header.frameCount; i++) {
  game.stepSimulation();

  if (i % everyN === 0 || i === tape.header.frameCount) {
    console.log(JSON.stringify({
      frame: i,
      rng: game.getRngState() >>> 0,
      score: game.getScore(),
      lives: game.getLives(),
      wave: game.getWave(),
    }));
  }
}
