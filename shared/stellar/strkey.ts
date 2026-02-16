import { Address, StrKey } from "@stellar/stellar-sdk";

export type ClaimantStrKeyType = "account" | "contract";

export function normalizeClaimantStrKeyInput(value: string): string {
  // StrKey values are base32 and case-insensitive; canonicalize for display + storage.
  return value.trim().toUpperCase();
}

export function parseClaimantStrKey(value: string): {
  normalized: string;
  type: ClaimantStrKeyType;
} {
  // Strict parser: expects already-normalized value.
  // Address.fromString validates checksum and format for both G... and C....
  try {
    Address.fromString(value);
  } catch {
    throw new Error("claimant address must be a valid Stellar G... or C... address");
  }

  if (StrKey.isValidEd25519PublicKey(value)) {
    return { normalized: value, type: "account" };
  }

  if (StrKey.isValidContract(value)) {
    return { normalized: value, type: "contract" };
  }

  throw new Error("claimant address must be a valid Stellar G... or C... address");
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
