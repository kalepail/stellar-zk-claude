import type { ProofJournal } from "./api";

export interface StoredProofArtifactEnvelope {
  stored_at?: string;
  prover_response?: unknown;
}

interface Groth16ReceiptLike {
  inner?: {
    Groth16?: {
      seal?: unknown;
      verifier_parameters?: unknown;
    };
  };
}

function asObject(value: unknown): Record<string, unknown> | null {
  if (!value || typeof value !== "object") {
    return null;
  }
  return value as Record<string, unknown>;
}

function asByte(value: unknown, fieldName: string, index: number): number {
  if (typeof value !== "number" || !Number.isFinite(value) || value < 0 || value > 255) {
    throw new Error(`${fieldName}[${index}] must be a byte`);
  }
  return value & 0xff;
}

function asU32(value: unknown, fieldName: string, index: number): number {
  if (typeof value !== "number" || !Number.isFinite(value) || value < 0 || value > 0xffff_ffff) {
    throw new Error(`${fieldName}[${index}] must be a u32`);
  }
  return value >>> 0;
}

function readGroth16Receipt(root: unknown): { seal: number[]; verifierParameters: number[] } {
  const parsed = root as Groth16ReceiptLike;
  const receipt = parsed?.inner?.Groth16;
  if (!receipt) {
    throw new Error("proof artifact is missing receipt.inner.Groth16");
  }

  if (!Array.isArray(receipt.seal)) {
    throw new Error("receipt.inner.Groth16.seal must be an array");
  }
  if (!Array.isArray(receipt.verifier_parameters)) {
    throw new Error("receipt.inner.Groth16.verifier_parameters must be an array");
  }
  if (receipt.seal.length !== 256) {
    throw new Error(`receipt.inner.Groth16.seal must have 256 bytes (got ${receipt.seal.length})`);
  }
  if (receipt.verifier_parameters.length !== 8) {
    throw new Error(
      `receipt.inner.Groth16.verifier_parameters must have 8 u32 words (got ${receipt.verifier_parameters.length})`,
    );
  }

  return {
    seal: receipt.seal.map((value, index) => asByte(value, "receipt.inner.Groth16.seal", index)),
    verifierParameters: receipt.verifier_parameters.map((value, index) =>
      asU32(value, "receipt.inner.Groth16.verifier_parameters", index),
    ),
  };
}

export function extractGroth16SealFromArtifact(artifact: unknown): Uint8Array {
  const artifactObj = asObject(artifact);
  if (!artifactObj) {
    throw new Error("proof artifact payload must be an object");
  }

  const proverResponse = asObject(artifactObj.prover_response);
  const result = proverResponse ? asObject(proverResponse.result) : null;
  const proof = result ? asObject(result.proof) : null;
  const receipt = proof?.receipt;

  const { seal: rawSeal, verifierParameters } = readGroth16Receipt(receipt);

  // `verifier_parameters` is [u32; 8] (digest words, little-endian). The selector
  // for Stellar verifier dispatch is the first 4 bytes of that digest.
  const paramsBytes = new Uint8Array(32);
  const paramsView = new DataView(paramsBytes.buffer);
  for (let index = 0; index < verifierParameters.length; index += 1) {
    paramsView.setUint32(index * 4, verifierParameters[index], true);
  }

  const selector = paramsBytes.slice(0, 4);
  const finalSeal = new Uint8Array(260);
  finalSeal.set(selector, 0);
  finalSeal.set(Uint8Array.from(rawSeal), 4);
  return finalSeal;
}

export function packJournalRaw(journal: ProofJournal): Uint8Array {
  const bytes = new Uint8Array(24);
  const view = new DataView(bytes.buffer);
  view.setUint32(0, journal.seed >>> 0, true);
  view.setUint32(4, journal.frame_count >>> 0, true);
  view.setUint32(8, journal.final_score >>> 0, true);
  view.setUint32(12, journal.final_rng_state >>> 0, true);
  view.setUint32(16, journal.tape_checksum >>> 0, true);
  view.setUint32(20, journal.rules_digest >>> 0, true);
  return bytes;
}
