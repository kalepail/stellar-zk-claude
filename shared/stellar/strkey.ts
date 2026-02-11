const STRKEY_LEN = 56;

// Stellar StrKey version bytes (before base32 encoding).
// See: https://github.com/stellar/stellar-protocol/blob/master/core/cap-0027.md
const VERSION_BYTE_ACCOUNT_ID = 48; // 'G...'
const VERSION_BYTE_CONTRACT_ID = 16; // 'C...'

const BASE32_CHAR_TO_VALUE = (() => {
  const map = new Int8Array(128);
  map.fill(-1);

  const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
  for (let i = 0; i < alphabet.length; i += 1) {
    map[alphabet.charCodeAt(i)] = i;
  }

  return map;
})();

export type ClaimantStrKeyType = "account" | "contract";

export function normalizeClaimantStrKeyInput(value: string): string {
  // Normalize user input for interoperability: StrKey is case-insensitive base32,
  // but we canonicalize to uppercase before strict decoding + checksum checks.
  return value.trim().toUpperCase();
}

export function parseClaimantStrKey(value: string): {
  normalized: string;
  type: ClaimantStrKeyType;
} {
  // Strict: StrKey is ASCII base32 without whitespace. Call
  // normalizeClaimantStrKeyInput() separately for user-input convenience.
  const normalized = value;

  if (normalized.length !== STRKEY_LEN) {
    throw new Error(`claimant address must be ${STRKEY_LEN} chars (got ${normalized.length})`);
  }

  const first = normalized[0];
  const type: ClaimantStrKeyType =
    first === "G" ? "account" : first === "C" ? "contract" : (() => {
      throw new Error(`claimant address must start with 'G' (account) or 'C' (contract) (got '${first}')`);
    })();

  const expectedVersionByte =
    type === "account" ? VERSION_BYTE_ACCOUNT_ID : VERSION_BYTE_CONTRACT_ID;

  // Raw = version (1) + payload (32) + crc16 (2) = 35 bytes.
  const raw = base32DecodeNoPad(normalized);
  if (raw.length !== 35) {
    throw new Error(`strkey decode produced ${raw.length} bytes (expected 35)`);
  }

  if (raw[0] !== expectedVersionByte) {
    throw new Error(
      `strkey version byte mismatch: got ${raw[0]}, expected ${expectedVersionByte} for '${first}' address`,
    );
  }

  const payload = raw.subarray(0, raw.length - 2);
  const storedCrc = raw[raw.length - 2] | (raw[raw.length - 1] << 8); // little-endian
  const computedCrc = crc16Xmodem(payload);

  if (storedCrc !== computedCrc) {
    throw new Error("strkey checksum mismatch");
  }

  return { normalized, type };
}

export function validateClaimantStrKey(value: string): void {
  void parseClaimantStrKey(value);
}

export function parseClaimantStrKeyFromUserInput(value: string): {
  normalized: string;
  type: ClaimantStrKeyType;
} {
  const normalized = normalizeClaimantStrKeyInput(value);
  return parseClaimantStrKey(normalized);
}

export function validateClaimantStrKeyFromUserInput(value: string): string {
  return parseClaimantStrKeyFromUserInput(value).normalized;
}

function base32DecodeNoPad(input: string): Uint8Array {
  let buffer = 0;
  let bits = 0;
  const out: number[] = [];

  for (let i = 0; i < input.length; i += 1) {
    const code = input.charCodeAt(i);
    if (code > 127) {
      throw new Error(`strkey contains non-ASCII character at index ${i}`);
    }

    const value = BASE32_CHAR_TO_VALUE[code];
    if (value < 0) {
      throw new Error(`strkey contains invalid base32 character '${input[i]}' at index ${i}`);
    }

    buffer = (buffer << 5) | value;
    bits += 5;

    while (bits >= 8) {
      bits -= 8;
      out.push((buffer >>> bits) & 0xff);
      buffer &= (1 << bits) - 1;
    }
  }

  // For 56-char StrKeys, bits ends at 0. If it's not 0, the input length is wrong
  // or has non-canonical trailing bits.
  if (bits !== 0) {
    throw new Error("strkey base32 has non-zero trailing bits");
  }

  return new Uint8Array(out);
}

function crc16Xmodem(data: Uint8Array): number {
  let crc = 0;
  for (let i = 0; i < data.length; i += 1) {
    crc ^= data[i] << 8;
    for (let bit = 0; bit < 8; bit += 1) {
      crc = (crc & 0x8000) !== 0 ? ((crc << 1) ^ 0x1021) : (crc << 1);
      crc &= 0xffff;
    }
  }
  return crc;
}
