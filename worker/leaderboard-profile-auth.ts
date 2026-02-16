import type { AuthenticatorTransportFuture } from "@simplewebauthn/server";

const BASE64URL_PATTERN = /^[A-Za-z0-9_-]+$/u;

const COSE_KEY_TYPE_EC2 = 2;
const COSE_ALG_ES256 = -7;
const COSE_CRV_P256 = 1;

export const DEFAULT_SMART_ACCOUNT_INDEXER_URL =
  "https://smart-account-indexer.sdf-ecosystem.workers.dev";
export const DEFAULT_SMART_ACCOUNT_INDEXER_TIMEOUT_MS = 10_000;

const VALID_AUTHENTICATOR_TRANSPORTS = new Set<AuthenticatorTransportFuture>([
  "ble",
  "cable",
  "hybrid",
  "internal",
  "nfc",
  "smart-card",
  "usb",
]);

export class LeaderboardCredentialBindingError extends Error {
  readonly retryable: boolean;
  readonly statusCode: number;

  constructor(
    message: string,
    { retryable, statusCode }: { retryable: boolean; statusCode: number },
  ) {
    super(message);
    this.name = "LeaderboardCredentialBindingError";
    this.retryable = retryable;
    this.statusCode = statusCode;
  }
}

function decodeBase64UrlString(value: string): Uint8Array {
  const normalized = value.trim();
  if (normalized.length === 0) {
    throw new Error("value must be a non-empty base64url string");
  }
  if (!BASE64URL_PATTERN.test(normalized)) {
    throw new Error("value must be base64url-encoded");
  }

  const padded =
    normalized.replace(/-/g, "+").replace(/_/g, "/") +
    "=".repeat((4 - (normalized.length % 4)) % 4);

  let binary: string;
  try {
    binary = atob(padded);
  } catch {
    throw new Error("value must be a valid base64url string");
  }

  const output = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    output[index] = binary.charCodeAt(index) & 0xff;
  }
  return output;
}

export function base64UrlToHex(value: string): string {
  const bytes = decodeBase64UrlString(value);
  let hex = "";
  for (const byte of bytes) {
    hex += byte.toString(16).padStart(2, "0");
  }
  return hex;
}

function appendCborTypeAndValue(output: number[], majorType: number, value: number): void {
  if (!Number.isInteger(majorType) || majorType < 0 || majorType > 7) {
    throw new Error("invalid CBOR major type");
  }
  if (!Number.isInteger(value) || value < 0) {
    throw new Error("invalid CBOR value");
  }

  const major = majorType << 5;
  if (value < 24) {
    output.push(major | value);
    return;
  }
  if (value < 256) {
    output.push(major | 24, value);
    return;
  }
  if (value < 65_536) {
    output.push(major | 25, (value >> 8) & 0xff, value & 0xff);
    return;
  }
  if (value <= 0xffff_ffff) {
    output.push(
      major | 26,
      (value >>> 24) & 0xff,
      (value >>> 16) & 0xff,
      (value >>> 8) & 0xff,
      value & 0xff,
    );
    return;
  }

  throw new Error("CBOR value exceeds supported integer range");
}

function appendCborInteger(output: number[], value: number): void {
  if (!Number.isInteger(value)) {
    throw new Error("CBOR integer value must be an integer");
  }

  if (value >= 0) {
    appendCborTypeAndValue(output, 0, value);
    return;
  }

  appendCborTypeAndValue(output, 1, -1 - value);
}

function appendCborBytes(output: number[], bytes: Uint8Array): void {
  appendCborTypeAndValue(output, 2, bytes.byteLength);
  for (const byte of bytes) {
    output.push(byte);
  }
}

export function encodeRawP256PublicKeyBase64UrlToCose(rawPublicKeyBase64Url: string): Uint8Array {
  const raw = decodeBase64UrlString(rawPublicKeyBase64Url);
  if (raw.byteLength !== 65 || raw[0] !== 0x04) {
    throw new Error("credential_public_key must be a 65-byte uncompressed secp256r1 key");
  }

  const x = raw.slice(1, 33);
  const y = raw.slice(33, 65);

  const cbor: number[] = [];
  appendCborTypeAndValue(cbor, 5, 5);
  appendCborInteger(cbor, 1);
  appendCborInteger(cbor, COSE_KEY_TYPE_EC2);
  appendCborInteger(cbor, 3);
  appendCborInteger(cbor, COSE_ALG_ES256);
  appendCborInteger(cbor, -1);
  appendCborInteger(cbor, COSE_CRV_P256);
  appendCborInteger(cbor, -2);
  appendCborBytes(cbor, x);
  appendCborInteger(cbor, -3);
  appendCborBytes(cbor, y);
  return new Uint8Array(cbor);
}

export function normalizeAuthenticatorTransports(
  raw: unknown,
): AuthenticatorTransportFuture[] | null {
  if (raw === null || raw === undefined) {
    return null;
  }
  if (!Array.isArray(raw)) {
    throw new Error("transports must be an array of strings");
  }
  if (raw.length > 8) {
    throw new Error("transports must contain at most 8 entries");
  }

  const deduped: AuthenticatorTransportFuture[] = [];
  const seen = new Set<string>();
  for (const entry of raw) {
    if (typeof entry !== "string") {
      throw new Error("transports must contain only strings");
    }

    const normalized = entry.trim();
    if (!VALID_AUTHENTICATOR_TRANSPORTS.has(normalized as AuthenticatorTransportFuture)) {
      throw new Error(`unsupported authenticator transport: ${normalized}`);
    }
    if (!seen.has(normalized)) {
      seen.add(normalized);
      deduped.push(normalized as AuthenticatorTransportFuture);
    }
  }

  return deduped;
}

function normalizeIndexerBaseUrl(rawBaseUrl: string | null | undefined): string {
  const baseUrl = rawBaseUrl?.trim() ?? "";
  if (baseUrl.length === 0) {
    return DEFAULT_SMART_ACCOUNT_INDEXER_URL;
  }
  return baseUrl.replace(/\/+$/u, "");
}

export async function fetchIndexedContractsForCredential({
  credentialIdBase64Url,
  baseUrl,
  timeoutMs = DEFAULT_SMART_ACCOUNT_INDEXER_TIMEOUT_MS,
  fetchImpl = fetch,
}: {
  credentialIdBase64Url: string;
  baseUrl?: string | null;
  timeoutMs?: number;
  fetchImpl?: typeof fetch;
}): Promise<string[]> {
  const credentialIdHex = base64UrlToHex(credentialIdBase64Url);
  const resolvedBaseUrl = normalizeIndexerBaseUrl(baseUrl);
  const controller = new AbortController();
  const timeoutId = setTimeout(() => controller.abort(), Math.max(1000, timeoutMs));

  try {
    const response = await fetchImpl(`${resolvedBaseUrl}/api/lookup/${credentialIdHex}`, {
      method: "GET",
      headers: {
        accept: "application/json",
      },
      signal: controller.signal,
    });

    if (response.status === 404) {
      return [];
    }
    if (!response.ok) {
      throw new LeaderboardCredentialBindingError(`indexer lookup failed (${response.status})`, {
        retryable: response.status >= 500 || response.status === 429,
        statusCode: 503,
      });
    }

    let payload: unknown;
    try {
      payload = await response.json();
    } catch {
      throw new LeaderboardCredentialBindingError("indexer returned malformed JSON", {
        retryable: true,
        statusCode: 503,
      });
    }

    if (!payload || typeof payload !== "object" || Array.isArray(payload)) {
      throw new LeaderboardCredentialBindingError("indexer returned unexpected payload", {
        retryable: true,
        statusCode: 503,
      });
    }

    const contractsRaw = (payload as Record<string, unknown>).contracts;
    if (!Array.isArray(contractsRaw)) {
      throw new LeaderboardCredentialBindingError("indexer payload missing contracts array", {
        retryable: true,
        statusCode: 503,
      });
    }

    const contracts: string[] = [];
    for (const contract of contractsRaw) {
      if (!contract || typeof contract !== "object" || Array.isArray(contract)) {
        continue;
      }
      const contractId = (contract as Record<string, unknown>).contract_id;
      if (typeof contractId === "string" && contractId.length > 0) {
        contracts.push(contractId);
      }
    }
    return contracts;
  } catch (error) {
    if (error instanceof LeaderboardCredentialBindingError) {
      throw error;
    }

    if (error instanceof DOMException && error.name === "AbortError") {
      throw new LeaderboardCredentialBindingError("indexer lookup timed out", {
        retryable: true,
        statusCode: 503,
      });
    }
    throw new LeaderboardCredentialBindingError(
      `indexer lookup failed: ${error instanceof Error ? error.message : String(error)}`,
      {
        retryable: true,
        statusCode: 503,
      },
    );
  } finally {
    clearTimeout(timeoutId);
  }
}

export async function assertCredentialBelongsToClaimantContract({
  claimantAddress,
  credentialIdBase64Url,
  indexerBaseUrl,
  timeoutMs,
  fetchImpl,
}: {
  claimantAddress: string;
  credentialIdBase64Url: string;
  indexerBaseUrl?: string | null;
  timeoutMs?: number;
  fetchImpl?: typeof fetch;
}): Promise<void> {
  if (!claimantAddress.startsWith("C")) {
    throw new LeaderboardCredentialBindingError(
      "profile updates require a smart-account contract claimant address",
      {
        retryable: false,
        statusCode: 403,
      },
    );
  }

  const contracts = await fetchIndexedContractsForCredential({
    credentialIdBase64Url,
    baseUrl: indexerBaseUrl,
    timeoutMs,
    fetchImpl,
  });
  if (!contracts.some((contractId) => contractId === claimantAddress)) {
    throw new LeaderboardCredentialBindingError(
      "credential is not linked to claimant address in smart-account indexer",
      {
        retryable: false,
        statusCode: 403,
      },
    );
  }
}
