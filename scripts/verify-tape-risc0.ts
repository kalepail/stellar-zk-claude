/**
 * RISC0 tape verification bridge.
 *
 * Usage:
 *   bun run scripts/verify-tape-risc0.ts --tape <path> [--max-frames <n>] [--segment-limit-po2 <n>] [--receipt-kind <kind>] [--claimant-address <strkey>] [--journal-out <path>]
 *
 * Local policy is enforced: dev proof mode only (RISC0_DEV_MODE=1, --proof-mode dev).
 */

import { spawnSync } from "node:child_process";
import { resolve } from "node:path";

const DEFAULT_CLAIMANT_ADDRESS = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

let tapePath = "";
let maxFrames = 18_000;
let journalOut = "";
let segmentLimitPo2 = 19;
let receiptKind = "composite";
let claimantAddress = DEFAULT_CLAIMANT_ADDRESS;
const usage =
  "Usage: bun run scripts/verify-tape-risc0.ts --tape <path> [--max-frames <n>] [--segment-limit-po2 <n>] [--receipt-kind <kind>] [--claimant-address <strkey>] [--journal-out <path>] (dev mode only; default --segment-limit-po2 19)";

const args = process.argv.slice(2);
for (let i = 0; i < args.length; i++) {
  const arg = args[i];

  if (arg === "--tape") {
    const value = args[++i];
    if (!value) {
      console.error("Missing value for --tape");
      console.error(usage);
      process.exit(1);
    }
    tapePath = value;
  } else if (arg === "--max-frames") {
    const value = args[++i];
    if (!value) {
      console.error("Missing value for --max-frames");
      console.error(usage);
      process.exit(1);
    }
    maxFrames = Number.parseInt(value, 10);
  } else if (arg === "--segment-limit-po2") {
    const value = args[++i];
    if (!value) {
      console.error("Missing value for --segment-limit-po2");
      console.error(usage);
      process.exit(1);
    }
    segmentLimitPo2 = Number.parseInt(value, 10);
  } else if (arg === "--receipt-kind") {
    const value = args[++i];
    if (!value) {
      console.error("Missing value for --receipt-kind");
      console.error(usage);
      process.exit(1);
    }
    receiptKind = value;
  } else if (arg === "--claimant-address") {
    const value = args[++i];
    if (!value) {
      console.error("Missing value for --claimant-address");
      console.error(usage);
      process.exit(1);
    }
    claimantAddress = value;
  } else if (arg === "--journal-out") {
    const value = args[++i];
    if (!value) {
      console.error("Missing value for --journal-out");
      console.error(usage);
      process.exit(1);
    }
    journalOut = value;
  } else if (arg === "-h" || arg === "--help") {
    console.log(usage);
    process.exit(0);
  } else {
    console.error(`Unknown option: ${arg}`);
    console.error(usage);
    process.exit(1);
  }
}

if (!["composite", "succinct", "groth16"].includes(receiptKind)) {
  console.error("Invalid --receipt-kind. Expected composite|succinct|groth16.");
  process.exit(1);
}

if (!tapePath) {
  console.error(usage);
  process.exit(1);
}

if (!Number.isInteger(maxFrames) || maxFrames < 1) {
  console.error("Invalid --max-frames. Expected integer >= 1.");
  process.exit(1);
}

if (!Number.isInteger(segmentLimitPo2) || segmentLimitPo2 < 1) {
  console.error("Invalid --segment-limit-po2. Expected integer >= 1.");
  process.exit(1);
}
if (!claimantAddress.trim()) {
  console.error("Invalid --claimant-address. Expected non-empty strkey.");
  process.exit(1);
}

const resolvedTape = resolve(process.cwd(), tapePath);
const resolvedJournal = journalOut ? resolve(process.cwd(), journalOut) : "";

const hostArgs = [
  "run",
  "-p",
  "host",
  "--release",
  "--no-default-features",
  "--",
  "--tape",
  resolvedTape,
  "--max-frames",
  String(maxFrames),
  "--receipt-kind",
  receiptKind,
  "--claimant-address",
  claimantAddress.trim(),
];

if (resolvedJournal) {
  hostArgs.push("--journal-out", resolvedJournal);
}
hostArgs.push("--segment-limit-po2", String(segmentLimitPo2));
hostArgs.push("--proof-mode", "dev");

const result = spawnSync("cargo", hostArgs, {
  cwd: resolve(process.cwd(), "risc0-asteroids-verifier"),
  stdio: "inherit",
  env: {
    ...process.env,
    RISC0_DEV_MODE: "1",
  },
});

if (result.error) {
  console.error(result.error.message);
  process.exit(1);
}

process.exit(result.status ?? 1);
