/**
 * Headless tape generator using autopilot.
 *
 * Usage: bun run scripts/generate-tape.ts [--seed <hex>] [--max-frames <n>] [--claimant <strkey>] [--output <path>]
 *
 * Runs an autopilot game in headless mode, records inputs to a tape,
 * writes the tape to a file, then verifies it inline.
 */

import { writeFileSync, readFileSync } from "fs";
import { AsteroidsGame } from "../src/game/AsteroidsGame";
import { TapeInputSource } from "../src/game/input-source";
import { Autopilot } from "../src/game/Autopilot";
import { deserializeTape } from "../src/game/tape";

const DEFAULT_MAX_FRAMES = 18_000;

// Parse arguments
let seed = Date.now();
let maxFrames = DEFAULT_MAX_FRAMES; // ~5 minutes
let outputPath = "";
let claimant = "";

const args = process.argv.slice(2);
for (let i = 0; i < args.length; i++) {
  if (args[i] === "--seed" && args[i + 1]) {
    seed = parseInt(args[++i], 16);
  } else if (args[i] === "--max-frames" && args[i + 1]) {
    maxFrames = parseInt(args[++i], 10);
  } else if (args[i] === "--output" && args[i + 1]) {
    outputPath = args[++i];
  } else if (args[i] === "--claimant" && args[i + 1]) {
    claimant = args[++i];
  }
}

if (!outputPath) {
  const seedHex = seed.toString(16).padStart(8, "0");
  outputPath = `asteroids-${seedHex}.tape`;
}

console.log(`Generating tape:`);
console.log(`  Seed:       0x${seed.toString(16).padStart(8, "0")}`);
console.log(`  Max frames: ${maxFrames}`);
console.log(`  Claimant:   ${claimant || "(none)"}`);
console.log(`  Output:     ${outputPath}`);
console.log();

// Create headless game and start with the given seed
const game = new AsteroidsGame({ headless: true, seed });
game.startNewGame(seed);

// Enable the internal autopilot (pragmatic private access for script use)
(game as unknown as { autopilot: Autopilot }).autopilot.setEnabled(true);

const start = performance.now();
let frame = 0;

while (frame < maxFrames) {
  game.stepSimulation();
  frame++;

  if (game.getMode() === "game-over") {
    break;
  }

  if (frame % 3000 === 0) {
    const elapsed = performance.now() - start;
    console.log(
      `  Frame ${frame}/${maxFrames} (score: ${game.getScore()}, wave: ${game.getWave()}, ${(frame / (elapsed / 1000)).toFixed(0)} fps)`,
    );
  }
}

const elapsed = performance.now() - start;

console.log();
console.log(`Generation complete:`);
console.log(`  Frames: ${frame}`);
console.log(`  Score:  ${game.getScore()}`);
console.log(`  Wave:   ${game.getWave()}`);
console.log(`  Lives:  ${game.getLives()}`);
console.log(`  Time:   ${elapsed.toFixed(1)}ms (${(frame / (elapsed / 1000)).toFixed(0)} fps)`);

const tapeData = game.getTape(claimant);
if (!tapeData) {
  console.error("Failed to get tape data");
  process.exit(1);
}

writeFileSync(outputPath, tapeData);
console.log(`  Written: ${outputPath} (${tapeData.length} bytes)`);

// Inline verification
console.log();
console.log("Verifying tape...");

const verifyData = new Uint8Array(readFileSync(outputPath));
const tape = deserializeTape(verifyData, DEFAULT_MAX_FRAMES);

const verifyGame = new AsteroidsGame({ headless: true, seed: tape.header.seed });
verifyGame.startNewGame(tape.header.seed);
const verifySource = new TapeInputSource(tape.inputs);
verifyGame.setInputSource(verifySource);

for (let i = 0; i < tape.header.frameCount; i++) {
  verifyGame.stepSimulation();
}

const vScore = verifyGame.getScore();
const vRng = verifyGame.getRngState();
const scoreOk = vScore === tape.footer.finalScore;
const rngOk = (vRng >>> 0) === (tape.footer.finalRngState >>> 0);

if (scoreOk && rngOk) {
  console.log("VERIFICATION PASSED");
} else {
  if (!scoreOk)
    console.error(`  Score mismatch: got ${vScore}, expected ${tape.footer.finalScore}`);
  if (!rngOk)
    console.error(
      `  RNG mismatch: got 0x${vRng.toString(16)}, expected 0x${tape.footer.finalRngState.toString(16)}`,
    );
  console.error("VERIFICATION FAILED");
  process.exit(1);
}
