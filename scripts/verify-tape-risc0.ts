/**
 * RISC0 tape verification bridge.
 *
 * Usage:
 *   bun run scripts/verify-tape-risc0.ts --tape <path> [--max-frames <n>] [--real] [--journal-out <path>]
 *
 * Defaults to RISC0_DEV_MODE=1 for fast iteration.
 */

import { spawnSync } from "node:child_process";
import { resolve } from "node:path";

let tapePath = "";
let maxFrames = 18_000;
let realProof = false;
let journalOut = "";

const args = process.argv.slice(2);
for (let i = 0; i < args.length; i++) {
  const arg = args[i];

  if (arg === "--tape" && args[i + 1]) {
    tapePath = args[++i];
  } else if (arg === "--max-frames" && args[i + 1]) {
    maxFrames = Number.parseInt(args[++i], 10);
  } else if (arg === "--real") {
    realProof = true;
  } else if (arg === "--journal-out" && args[i + 1]) {
    journalOut = args[++i];
  }
}

if (!tapePath) {
  console.error(
    "Usage: bun run scripts/verify-tape-risc0.ts --tape <path> [--max-frames <n>] [--real] [--journal-out <path>]",
  );
  process.exit(1);
}

const resolvedTape = resolve(process.cwd(), tapePath);
const resolvedJournal = journalOut ? resolve(process.cwd(), journalOut) : "";

const hostArgs = [
  "run",
  "-p",
  "host",
  "--release",
  "--",
  "--tape",
  resolvedTape,
  "--max-frames",
  String(maxFrames),
];

if (resolvedJournal) {
  hostArgs.push("--journal-out", resolvedJournal);
}

const result = spawnSync("cargo", hostArgs, {
  cwd: resolve(process.cwd(), "risc0-asteroids-verifier"),
  stdio: "inherit",
  env: {
    ...process.env,
    RISC0_DEV_MODE: realProof ? "0" : "1",
  },
});

if (result.error) {
  console.error(result.error.message);
  process.exit(1);
}

process.exit(result.status ?? 1);
