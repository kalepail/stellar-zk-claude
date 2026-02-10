/**
 * Replay Tape: binary format, recorder, serializer, and CRC-32.
 *
 * Tape layout (little-endian):
 *
 * HEADER (72 bytes):
 *   [0..3]   u32    magic           = 0x5A4B5450 ("ZKTP")
 *   [4]      u8     version         = 1
 *   [5]      u8     rules_tag       = 3
 *   [6..7]   u8[2]  reserved        = 0
 *   [8..11]  u32    seed
 *   [12..15] u32    frameCount
 *   [16..71] u8[56] claimantAddress (ASCII Stellar strkey, zero-padded)
 *
 * BODY (frameCount bytes):
 *   [72 .. 72+N-1]  u8[]  One byte per frame
 *     bit 0 (0x01): left
 *     bit 1 (0x02): right
 *     bit 2 (0x04): thrust
 *     bit 3 (0x08): fire
 *     bits 4-7: reserved (0)
 *
 * FOOTER (12 bytes):
 *   [72+N .. 72+N+3]   u32  finalScore
 *   [72+N+4 .. 72+N+7] u32  finalRngState
 *   [72+N+8 .. 72+N+11] u32  checksum (CRC-32 of header+body)
 */

import { RULES_TAG } from "./constants";

export const TAPE_MAGIC = 0x5a4b5450;
export const TAPE_VERSION = 1;

const HEADER_SIZE = 72;
const FOOTER_SIZE = 12;
const CLAIMANT_ADDRESS_SIZE = 56;

export interface TapeHeader {
  magic: number;
  version: number;
  rulesTag: number;
  seed: number;
  frameCount: number;
  claimantAddress: string;
}

export interface TapeFooter {
  finalScore: number;
  finalRngState: number;
  checksum: number;
}

export interface Tape {
  header: TapeHeader;
  inputs: Uint8Array;
  footer: TapeFooter;
}

export interface FrameInput {
  left: boolean;
  right: boolean;
  thrust: boolean;
  fire: boolean;
}

export function encodeInputByte(input: FrameInput): number {
  return (
    (input.left ? 0x01 : 0) |
    (input.right ? 0x02 : 0) |
    (input.thrust ? 0x04 : 0) |
    (input.fire ? 0x08 : 0)
  );
}

export function decodeInputByte(byte: number): FrameInput {
  return {
    left: (byte & 0x01) !== 0,
    right: (byte & 0x02) !== 0,
    thrust: (byte & 0x04) !== 0,
    fire: (byte & 0x08) !== 0,
  };
}

const INITIAL_CAPACITY = 18000; // ~5 minutes at 60fps

export class TapeRecorder {
  private buffer: Uint8Array;
  private cursor = 0;

  constructor() {
    this.buffer = new Uint8Array(INITIAL_CAPACITY);
  }

  record(input: FrameInput): void {
    if (this.cursor >= this.buffer.length) {
      const next = new Uint8Array(this.buffer.length * 2);
      next.set(this.buffer);
      this.buffer = next;
    }
    this.buffer[this.cursor++] = encodeInputByte(input);
  }

  getInputs(): Uint8Array {
    return this.buffer.subarray(0, this.cursor);
  }

  getFrameCount(): number {
    return this.cursor;
  }
}

export function serializeTape(
  seed: number,
  inputs: Uint8Array,
  finalScore: number,
  finalRngState: number,
  claimantAddress = "",
): Uint8Array {
  const frameCount = inputs.length;
  const totalSize = HEADER_SIZE + frameCount + FOOTER_SIZE;
  const data = new Uint8Array(totalSize);
  const view = new DataView(data.buffer);

  // Header
  view.setUint32(0, TAPE_MAGIC, true);
  view.setUint8(4, TAPE_VERSION);
  view.setUint8(5, RULES_TAG);
  // reserved bytes 6-7 already 0
  view.setUint32(8, seed >>> 0, true);
  view.setUint32(12, frameCount, true);

  // Claimant address: 56 bytes ASCII, zero-padded
  const encoder = new TextEncoder();
  const claimantBytes = encoder.encode(claimantAddress);
  const claimantLen = Math.min(claimantBytes.length, CLAIMANT_ADDRESS_SIZE);
  data.set(claimantBytes.subarray(0, claimantLen), 16);

  // Body
  data.set(inputs, HEADER_SIZE);

  // Footer
  const footerOffset = HEADER_SIZE + frameCount;
  view.setUint32(footerOffset, finalScore >>> 0, true);
  view.setUint32(footerOffset + 4, finalRngState >>> 0, true);

  // CRC-32 over header + body
  const checksum = crc32(data.subarray(0, footerOffset));
  view.setUint32(footerOffset + 8, checksum >>> 0, true);

  return data;
}

export function deserializeTape(data: Uint8Array, maxFrames?: number): Tape {
  if (data.length < HEADER_SIZE + FOOTER_SIZE) {
    throw new Error("Tape too short");
  }

  const view = new DataView(data.buffer, data.byteOffset, data.byteLength);

  const magic = view.getUint32(0, true);
  if (magic !== TAPE_MAGIC) {
    throw new Error(`Invalid tape magic: 0x${magic.toString(16)}`);
  }

  const version = view.getUint8(4);
  if (version !== TAPE_VERSION) {
    throw new Error(`Unsupported tape version: ${version}`);
  }

  const rulesTag = view.getUint8(5);
  if (rulesTag !== 0 && rulesTag !== RULES_TAG) {
    throw new Error(`Unknown rules tag: ${rulesTag}`);
  }
  if (view.getUint8(6) !== 0 || view.getUint8(7) !== 0) {
    throw new Error("Header reserved bytes [6..7] are non-zero");
  }

  const seed = view.getUint32(8, true);
  const frameCount = view.getUint32(12, true);
  if (frameCount === 0 || (maxFrames !== undefined && frameCount > maxFrames)) {
    throw new Error(
      `Frame count out of range: ${frameCount}${maxFrames !== undefined ? ` (max ${maxFrames})` : ""}`,
    );
  }

  const expectedLength = HEADER_SIZE + frameCount + FOOTER_SIZE;
  if (data.length !== expectedLength) {
    throw new Error(`Tape length mismatch: expected ${expectedLength} bytes, got ${data.length}`);
  }

  // Read claimant address: 56 bytes at offset 16, trim trailing zeros
  const claimantRaw = data.subarray(16, 16 + CLAIMANT_ADDRESS_SIZE);
  let claimantEnd = claimantRaw.length;
  while (claimantEnd > 0 && claimantRaw[claimantEnd - 1] === 0) {
    claimantEnd--;
  }
  const claimantAddress = new TextDecoder().decode(claimantRaw.subarray(0, claimantEnd));

  const inputs = data.subarray(HEADER_SIZE, HEADER_SIZE + frameCount);

  const footerOffset = HEADER_SIZE + frameCount;
  const finalScore = view.getUint32(footerOffset, true);
  const finalRngState = view.getUint32(footerOffset + 4, true);
  const storedChecksum = view.getUint32(footerOffset + 8, true);

  // Verify CRC-32 and reserved input bits in one pass.
  const computed = crc32AndValidateInputs(data, HEADER_SIZE, footerOffset);
  if (computed >>> 0 !== storedChecksum >>> 0) {
    throw new Error(
      `CRC mismatch: stored=0x${storedChecksum.toString(16)}, computed=0x${(computed >>> 0).toString(16)}`,
    );
  }

  return {
    header: { magic, version, rulesTag, seed, frameCount, claimantAddress },
    inputs,
    footer: { finalScore, finalRngState, checksum: storedChecksum },
  };
}

// CRC-32 (ISO 3309 / ITU-T V.42 polynomial)
const CRC_TABLE = buildCrcTable();

function buildCrcTable(): Uint32Array {
  const table = new Uint32Array(256);
  for (let i = 0; i < 256; i++) {
    let c = i;
    for (let j = 0; j < 8; j++) {
      c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    }
    table[i] = c >>> 0;
  }
  return table;
}

export function crc32(data: Uint8Array): number {
  let crc = 0xffffffff;
  for (let i = 0; i < data.length; i++) {
    crc = CRC_TABLE[(crc ^ data[i]) & 0xff] ^ (crc >>> 8);
  }
  return (crc ^ 0xffffffff) >>> 0;
}

function crc32AndValidateInputs(data: Uint8Array, inputsStart: number, inputsEnd: number): number {
  let crc = 0xffffffff;
  for (let i = 0; i < inputsEnd; i++) {
    const byte = data[i];
    if (i >= inputsStart && (byte & 0xf0) !== 0) {
      const frame = i - inputsStart;
      throw new Error(
        `Input byte reserved bits set at frame ${frame}: 0x${byte.toString(16).padStart(2, "0")}`,
      );
    }
    crc = CRC_TABLE[(crc ^ byte) & 0xff] ^ (crc >>> 8);
  }
  return (crc ^ 0xffffffff) >>> 0;
}
