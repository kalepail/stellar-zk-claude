import {
  EXPECTED_RULES_TAG,
  TAPE_FOOTER_SIZE,
  TAPE_HEADER_SIZE,
  TAPE_MAGIC,
  TAPE_VERSION,
} from "./constants";
import type { TapeMetadata } from "./types";

function crc32AndValidateInputs(data: Uint8Array, inputsStart: number, inputsEnd: number): number {
  let crc = 0xffffffff;

  for (let index = 0; index < inputsEnd; index += 1) {
    const byte = data[index];
    if (index >= inputsStart && (byte & 0xf0) !== 0) {
      const frame = index - inputsStart;
      throw new Error(
        `tape input reserved bits set at frame ${frame}: 0x${byte.toString(16).padStart(2, "0")}`,
      );
    }
    crc = CRC_TABLE[(crc ^ byte) & 0xff] ^ (crc >>> 8);
  }

  return (crc ^ 0xffffffff) >>> 0;
}

export function parseAndValidateTape(bytes: Uint8Array, maxTapeBytes: number): TapeMetadata {
  if (bytes.length === 0) {
    throw new Error("tape payload is empty");
  }

  if (bytes.length > maxTapeBytes) {
    throw new Error(`tape payload too large: ${bytes.length} bytes (max ${maxTapeBytes})`);
  }

  if (bytes.length < TAPE_HEADER_SIZE + TAPE_FOOTER_SIZE) {
    throw new Error("tape payload is too short");
  }

  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const magic = view.getUint32(0, true);
  if (magic !== TAPE_MAGIC) {
    throw new Error(`invalid tape magic: 0x${magic.toString(16)}`);
  }

  const version = view.getUint8(4);
  if (version !== TAPE_VERSION) {
    throw new Error(`unsupported tape version: ${version}`);
  }

  const rulesTag = view.getUint8(5);
  if (rulesTag !== EXPECTED_RULES_TAG) {
    throw new Error(`unknown rules tag: ${rulesTag} (expected ${EXPECTED_RULES_TAG})`);
  }
  if (view.getUint8(6) !== 0 || view.getUint8(7) !== 0) {
    throw new Error("tape header reserved bytes [6..7] are non-zero");
  }

  const seed = view.getUint32(8, true);
  const frameCount = view.getUint32(12, true);
  const expectedLength = TAPE_HEADER_SIZE + frameCount + TAPE_FOOTER_SIZE;

  if (bytes.length !== expectedLength) {
    throw new Error(`tape size mismatch: expected ${expectedLength} bytes, got ${bytes.length}`);
  }

  const footerOffset = TAPE_HEADER_SIZE + frameCount;
  const finalScore = view.getUint32(footerOffset, true);
  const finalRngState = view.getUint32(footerOffset + 4, true);
  const checksum = view.getUint32(footerOffset + 8, true);

  if (finalScore === 0) {
    throw new Error("tape final_score must be greater than zero");
  }

  const computedChecksum = crc32AndValidateInputs(bytes, TAPE_HEADER_SIZE, footerOffset);
  if (checksum >>> 0 !== computedChecksum >>> 0) {
    throw new Error(
      `tape checksum mismatch: expected 0x${checksum.toString(16)}, computed 0x${computedChecksum.toString(16)}`,
    );
  }

  return {
    seed,
    frameCount,
    finalScore,
    finalRngState,
    checksum,
  };
}

const CRC_TABLE = (() => {
  const table = new Uint32Array(256);

  for (let i = 0; i < 256; i += 1) {
    let current = i;

    for (let bit = 0; bit < 8; bit += 1) {
      current = current & 1 ? 0xedb88320 ^ (current >>> 1) : current >>> 1;
    }

    table[i] = current >>> 0;
  }

  return table;
})();
