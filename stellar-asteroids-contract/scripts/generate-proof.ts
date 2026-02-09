#!/usr/bin/env bun
/**
 * generate-proof.ts
 *
 * Submits a tape to the RISC Zero prover API, polls for a Groth16 receipt,
 * and saves a proof fixture with the seal, journal, and image_id formatted
 * for the Soroban score contract.
 *
 * Usage:
 *   bun run scripts/generate-proof.ts \
 *     --tape ../test-fixtures/test-short.tape \
 *     --prover https://risc0-kalien.stellar.buzz \
 *     --out ../test-fixtures/proof-short.json
 *
 * Output fixture format:
 *   {
 *     "seal":        "hex string (260 bytes = 4-byte selector + 256-byte proof)",
 *     "journal_raw": "hex string (24 bytes = 6 x u32 LE)",
 *     "image_id":    "hex string (32 bytes LE)",
 *     "journal":     { seed, frame_count, final_score, ... },
 *     "receipt_kind": "groth16",
 *     "prover_stats": { ... }
 *   }
 */

import { readFileSync, writeFileSync } from "fs";
import { resolve } from "path";
import { createHash } from "crypto";

const EXPECTED_RULES_DIGEST = 0x41535432; // "AST2"

// ---------------------------------------------------------------------------
// CLI args
// ---------------------------------------------------------------------------

function parseArgs() {
  const args = process.argv.slice(2);
  let tape = "";
  let prover = "https://risc0-kalien.stellar.buzz";
  let out = "";
  let receiptKind = "groth16";
  let segmentLimitPo2 = "21";
  let maxFrames = "18000";
  let imageId = "";

  for (let i = 0; i < args.length; i++) {
    switch (args[i]) {
      case "--tape":
        tape = args[++i];
        break;
      case "--prover":
        prover = args[++i];
        break;
      case "--out":
        out = args[++i];
        break;
      case "--receipt-kind":
        receiptKind = args[++i];
        break;
      case "--segment-limit-po2":
        segmentLimitPo2 = args[++i];
        break;
      case "--max-frames":
        maxFrames = args[++i];
        break;
      case "--image-id":
        imageId = args[++i];
        break;
      default:
        console.error(`Unknown arg: ${args[i]}`);
        process.exit(1);
    }
  }

  if (!tape) {
    console.error(
      "Usage: bun run scripts/generate-proof.ts --tape <file.tape> [--prover <url>] [--out <file.json>]"
    );
    process.exit(1);
  }

  const tapePath = resolve(tape);
  if (!out) {
    out = tapePath.replace(/\.tape$/, `-proof-${receiptKind}.json`);
  } else {
    out = resolve(out);
  }

  return {
    tapePath,
    proverUrl: prover,
    outPath: out,
    receiptKind,
    segmentLimitPo2,
    maxFrames,
    imageId,
  };
}

// ---------------------------------------------------------------------------
// Prover API helpers
// ---------------------------------------------------------------------------

interface ProverCreateResponse {
  success: boolean;
  job_id: string;
  status: string;
  status_url: string;
  error?: string;
}

interface ProverJobResponse {
  success: boolean;
  status: "queued" | "running" | "succeeded" | "failed";
  error?: string;
  result?: {
    proof: {
      journal: {
        seed: number;
        frame_count: number;
        final_score: number;
        final_rng_state: number;
        tape_checksum: number;
        rules_digest: number;
      };
      receipt: any;
      requested_receipt_kind: string;
      produced_receipt_kind?: string | null;
      stats: {
        segments: number;
        total_cycles: number;
        user_cycles: number;
        paging_cycles: number;
        reserved_cycles: number;
      };
    };
    elapsed_ms: number;
  };
}

async function submitTape(
  proverUrl: string,
  tapeBytes: Uint8Array,
  receiptKind: string,
  segmentLimitPo2: string,
  maxFrames: string
): Promise<string> {
  const url = `${proverUrl}/api/jobs/prove-tape/raw?receipt_kind=${receiptKind}&segment_limit_po2=${segmentLimitPo2}&max_frames=${maxFrames}&verify_receipt=true`;

  console.log(`Submitting tape (${tapeBytes.length} bytes) to ${proverUrl}...`);

  const resp = await fetch(url, {
    method: "POST",
    headers: { "Content-Type": "application/octet-stream" },
    body: tapeBytes,
  });

  const body: ProverCreateResponse = await resp.json();

  if (!resp.ok || !body.success) {
    throw new Error(
      `Submit failed (${resp.status}): ${body.error || JSON.stringify(body)}`
    );
  }

  console.log(`Job created: ${body.job_id}`);
  return body.job_id;
}

async function pollJob(
  proverUrl: string,
  jobId: string
): Promise<ProverJobResponse> {
  const url = `${proverUrl}/api/jobs/${jobId}`;
  const pollIntervalMs = 3000;
  const maxPollMs = 20 * 60 * 1000; // 20 minutes for Groth16
  const startTime = Date.now();

  while (Date.now() - startTime < maxPollMs) {
    const resp = await fetch(url);
    const body: ProverJobResponse = await resp.json();

    if (body.status === "succeeded") {
      return body;
    }

    if (body.status === "failed") {
      throw new Error(`Proof failed: ${body.error || "unknown error"}`);
    }

    const elapsed = ((Date.now() - startTime) / 1000).toFixed(0);
    process.stdout.write(`\r  Proving... ${body.status} (${elapsed}s)`);
    await new Promise((r) => setTimeout(r, pollIntervalMs));
  }

  throw new Error("Proof timed out after 20 minutes");
}

// ---------------------------------------------------------------------------
// Seal extraction
// ---------------------------------------------------------------------------

function extractSeal(receipt: any): Uint8Array {
  // Groth16 receipt structure:
  //   receipt.inner.Groth16.seal -- 256 u8 values (raw proof: a || b || c)
  //   receipt.inner.Groth16.verifier_parameters -- [u32; 8] (Digest, 32 bytes LE)
  //
  // The selector is the first 4 bytes of the verifier_parameters digest (LE).
  const inner = receipt.inner;

  if (!inner || !inner.Groth16) {
    if (inner?.Succinct || inner?.Composite) {
      throw new Error(
        "Receipt is not Groth16 -- only Groth16 proofs can be verified on Stellar"
      );
    }
    throw new Error(
      `Unexpected receipt structure: ${JSON.stringify(Object.keys(inner || {}))}`
    );
  }

  const groth16 = inner.Groth16;
  const rawSeal: number[] = groth16.seal;
  const verifierParamsU32: number[] = groth16.verifier_parameters;

  if (rawSeal.length !== 256) {
    throw new Error(`Expected 256-byte seal, got ${rawSeal.length}`);
  }

  if (verifierParamsU32.length !== 8) {
    throw new Error(
      `Expected [u32; 8] verifier_parameters, got length ${verifierParamsU32.length}`
    );
  }

  // Convert [u32; 8] to 32 LE bytes
  const vpBytes = new Uint8Array(32);
  const vpView = new DataView(vpBytes.buffer);
  for (let i = 0; i < 8; i++) {
    vpView.setUint32(i * 4, verifierParamsU32[i], true);
  }

  // Selector = first 4 bytes of the digest
  const selector = vpBytes.slice(0, 4);

  // Final seal = selector (4) + raw proof (256) = 260 bytes
  const seal = new Uint8Array(260);
  seal.set(selector, 0);
  seal.set(rawSeal, 4);

  return seal;
}

function extractJournalRaw(journal: ProverJobResponse["result"]["proof"]["journal"]): Uint8Array {
  // Journal is 6 x u32 LE = 24 bytes
  const buf = new Uint8Array(24);
  const view = new DataView(buf.buffer);

  view.setUint32(0, journal.seed, true);
  view.setUint32(4, journal.frame_count, true);
  view.setUint32(8, journal.final_score, true);
  view.setUint32(12, journal.final_rng_state, true);
  view.setUint32(16, journal.tape_checksum, true);
  view.setUint32(20, journal.rules_digest, true);

  return buf;
}

async function fetchImageIdFromProver(proverUrl: string): Promise<Uint8Array> {
  console.log(`Fetching image_id from prover health endpoint...`);
  const resp = await fetch(`${proverUrl}/health`);
  if (!resp.ok) {
    throw new Error(`Failed to fetch /health: ${resp.status}`);
  }
  const health = (await resp.json()) as { image_id?: string };
  if (!health.image_id || health.image_id.length !== 64) {
    throw new Error(
      `Prover /health did not return a valid image_id (got: ${health.image_id ?? "missing"}). ` +
        "Update the prover to include image_id in the health response, or pass --image-id manually."
    );
  }
  console.log(`  image_id from prover: ${health.image_id}`);
  return new Uint8Array(Buffer.from(health.image_id, "hex"));
}

function extractImageId(imageIdOverride?: string): Uint8Array | null {
  // If --image-id is passed, use that. Otherwise return null to signal
  // that it should be fetched from the prover at runtime.
  if (imageIdOverride) {
    const hex = imageIdOverride.replace(/^0x/, "");
    if (hex.length !== 64) {
      throw new Error(`--image-id must be 32 bytes (64 hex chars), got ${hex.length}`);
    }
    return new Uint8Array(Buffer.from(hex, "hex"));
  }
  return null;
}

function toHex(bytes: Uint8Array): string {
  return Buffer.from(bytes).toString("hex");
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

async function main() {
  const config = parseArgs();

  // Read tape
  const tapeBytes = new Uint8Array(readFileSync(config.tapePath));
  console.log(`Tape: ${config.tapePath} (${tapeBytes.length} bytes)`);

  // Submit to prover
  const jobId = await submitTape(
    config.proverUrl,
    tapeBytes,
    config.receiptKind,
    config.segmentLimitPo2,
    config.maxFrames
  );

  // Poll for result
  const result = await pollJob(config.proverUrl, jobId);
  console.log(); // newline after progress

  if (!result.result) {
    throw new Error("No result in succeeded job");
  }

  const { proof } = result.result;
  const rulesDigest = proof.journal.rules_digest >>> 0;
  if (rulesDigest !== EXPECTED_RULES_DIGEST) {
    throw new Error(
      `Prover returned rules_digest=0x${rulesDigest.toString(16)}; expected 0x${EXPECTED_RULES_DIGEST.toString(16)} (AST2). Update/redeploy prover before generating fixtures.`
    );
  }

  console.log(`Proof complete in ${result.result.elapsed_ms}ms`);
  console.log(
    `  Receipt kind: ${proof.produced_receipt_kind || proof.requested_receipt_kind}`
  );
  console.log(`  Score: ${proof.journal.final_score}`);
  console.log(`  Frames: ${proof.journal.frame_count}`);
  console.log(`  Rules digest: 0x${rulesDigest.toString(16)}`);
  console.log(
    `  Cycles: ${proof.stats.total_cycles.toLocaleString()} (${proof.stats.segments} segments)`
  );

  // Extract on-chain data
  const seal = extractSeal(proof.receipt);
  const journalRaw = extractJournalRaw(proof.journal);

  // Get image_id: prefer --image-id flag, then fetch from prover /health
  let imageId = extractImageId(config.imageId || undefined);
  if (!imageId) {
    imageId = await fetchImageIdFromProver(config.proverUrl);
  }

  // Compute journal digest (what the verifier receives)
  const journalDigest = createHash("sha256").update(journalRaw).digest();

  console.log(`\nOn-chain data:`);
  console.log(`  Seal: ${seal.length} bytes`);
  console.log(`  Journal raw: ${journalRaw.length} bytes`);
  console.log(`  Journal digest: ${toHex(new Uint8Array(journalDigest))}`);
  console.log(`  Image ID: ${toHex(imageId)}`);

  // Write hex fixture files for Rust include_str! tests
  const basePath = config.outPath.replace(/\.json$/, "");
  writeFileSync(`${basePath}.seal`, toHex(seal));
  writeFileSync(`${basePath}.journal_raw`, toHex(journalRaw));
  writeFileSync(`${basePath}.image_id`, toHex(imageId));

  console.log(`\nFixture files written:`);
  console.log(`  ${basePath}.seal`);
  console.log(`  ${basePath}.journal_raw`);
  console.log(`  ${basePath}.image_id`);
}

main().catch((err) => {
  console.error(`\nError: ${err.message}`);
  process.exit(1);
});
