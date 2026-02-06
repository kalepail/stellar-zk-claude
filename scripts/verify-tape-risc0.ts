/**
 * RISC0 tape verification bridge.
 *
 * Usage:
 *   bun run scripts/verify-tape-risc0.ts --tape <path> [--max-frames <n>] [--dev|--real] [--segment-limit-po2 <n>] [--receipt-kind <kind>] [--journal-out <path>]
 *
 * Defaults to real proof mode (RISC0_DEV_MODE=0) and segment limit po2=19.
 */

import { spawnSync } from "node:child_process";
import { resolve } from "node:path";

let tapePath = "";
let maxFrames = 18_000;
let realProof = true;
let journalOut = "";
let segmentLimitPo2 = 19;
let receiptKind = "composite";

const args = process.argv.slice(2);
for (let i = 0; i < args.length; i++) {
  const arg = args[i];

  if (arg === "--tape" && args[i + 1]) {
    tapePath = args[++i];
  } else if (arg === "--max-frames" && args[i + 1]) {
    maxFrames = Number.parseInt(args[++i], 10);
  } else if (arg === "--dev") {
    realProof = false;
  } else if (arg === "--real") {
    realProof = true;
  } else if (arg === "--segment-limit-po2" && args[i + 1]) {
    segmentLimitPo2 = Number.parseInt(args[++i], 10);
  } else if (arg === "--receipt-kind" && args[i + 1]) {
    receiptKind = args[++i]!;
  } else if (arg === "--journal-out" && args[i + 1]) {
    journalOut = args[++i];
  }
}

if (!["composite", "succinct", "groth16"].includes(receiptKind)) {
  console.error("Invalid --receipt-kind. Expected composite|succinct|groth16.");
  process.exit(1);
}

if (!tapePath) {
  console.error(
    "Usage: bun run scripts/verify-tape-risc0.ts --tape <path> [--max-frames <n>] [--dev|--real] [--segment-limit-po2 <n>] [--receipt-kind <kind>] [--journal-out <path>] (default --segment-limit-po2 19)",
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
  "--receipt-kind",
  receiptKind,
];

if (resolvedJournal) {
  hostArgs.push("--journal-out", resolvedJournal);
}
hostArgs.push("--segment-limit-po2", String(segmentLimitPo2));
if (!realProof) {
  hostArgs.push("--allow-dev-mode");
  console.warn("WARNING: running with RISC0_DEV_MODE=1 (fake receipts for development only).");
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
