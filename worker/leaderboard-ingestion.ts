import { StrKey, scValToNative, xdr } from "@stellar/stellar-base";
import { decompress } from "fzstd";
import { validateClaimantStrKeyFromUserInput } from "../shared/stellar/strkey";
import type { WorkerEnv } from "./env";
import type { LeaderboardEventRecord } from "./types";
import { nowIso, parseInteger, safeErrorMessage } from "./utils";

const DEFAULT_GALEXIE_EVENTS_PATH = "/events";
const DEFAULT_GALEXIE_TIMEOUT_MS = 20_000;
const DEFAULT_GALEXIE_PAGE_LIMIT = 200;
const MAX_GALEXIE_PAGE_LIMIT = 1_000;

const DEFAULT_GALEXIE_DATA_ROOT_PATH = "/v1";
const DEFAULT_GALEXIE_OBJECT_EXTENSION = "zst";
const DEFAULT_FORWARD_LEDGER_WINDOW = 4_096;
const DEFAULT_RPC_BASE_URL = "https://rpc-pro.lightsail.network";
const SCORE_EVENT_NAMES = new Set(["score_submitted", "ScoreSubmitted", "scoreSubmitted"]);
const SCORE_EVENT_KEYS = new Set(["score_submitted", "scoresubmitted"]);

export type LeaderboardSourceMode = "auto" | "rpc" | "events_api" | "datalake";
export type LeaderboardResolvedSourceMode = Exclude<LeaderboardSourceMode, "auto">;
export type LeaderboardProvider = "rpc" | "galexie";

type JsonRecord = Record<string, unknown>;

type NativeMapLike = Record<string, unknown> | Map<unknown, unknown>;

export interface GalexieFetchOptions {
  cursor?: string | null;
  fromLedger?: number | null;
  toLedger?: number | null;
  limit?: number;
  source?: LeaderboardResolvedSourceMode | "default" | null;
}

export interface GalexieFetchResult {
  events: LeaderboardEventRecord[];
  nextCursor: string | null;
  fetchedCount: number;
  provider: LeaderboardProvider;
  sourceMode: LeaderboardResolvedSourceMode;
}

interface GalexieDatastoreConfig {
  networkPassphrase: string;
  ledgersPerBatch: number;
  batchesPerPartition: number;
  compression: string;
}

function asRecord(value: unknown): JsonRecord | null {
  return value && typeof value === "object" && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function pickValue(records: JsonRecord[], keys: string[]): unknown {
  for (const key of keys) {
    for (const record of records) {
      const value = record[key];
      if (value !== undefined) {
        return value;
      }
    }
  }
  return undefined;
}

function toInteger(value: unknown): number | null {
  if (typeof value === "number" && Number.isFinite(value)) {
    return Math.trunc(value);
  }

  if (typeof value === "bigint") {
    if (value < 0n || value > BigInt(Number.MAX_SAFE_INTEGER)) {
      return null;
    }
    return Number(value);
  }

  if (Array.isArray(value) && value.length > 0) {
    return toInteger(value[0]);
  }

  const recordValue = asRecord(value);
  if (recordValue) {
    const nested = pickValue([recordValue], ["u32", "value", "int", "number", "amount"]);
    if (nested !== undefined) {
      return toInteger(nested);
    }
  }

  if (typeof value === "string") {
    const trimmed = value.trim();
    if (trimmed.length === 0) {
      return null;
    }

    const radix = trimmed.startsWith("0x") || trimmed.startsWith("0X") ? 16 : 10;
    const parsed = Number.parseInt(trimmed, radix);
    if (Number.isFinite(parsed)) {
      return parsed;
    }
  }

  return null;
}

function toIsoTimestamp(value: unknown): string | null {
  if (typeof value === "number" && Number.isFinite(value)) {
    const ms = value > 1_000_000_000_000 ? value : value * 1_000;
    return new Date(ms).toISOString();
  }

  if (typeof value === "bigint") {
    const asNumber = Number(value);
    if (!Number.isFinite(asNumber)) {
      return null;
    }
    const ms = asNumber > 1_000_000_000_000 ? asNumber : asNumber * 1_000;
    return new Date(ms).toISOString();
  }

  if (Array.isArray(value) && value.length > 0) {
    return toIsoTimestamp(value[0]);
  }

  const recordValue = asRecord(value);
  if (recordValue) {
    const nested = pickValue([recordValue], ["timestamp", "time", "value", "seconds", "unix"]);
    if (nested !== undefined) {
      return toIsoTimestamp(nested);
    }
  }

  if (typeof value === "string" && value.trim().length > 0) {
    const parsed = new Date(value.trim()).getTime();
    if (Number.isFinite(parsed)) {
      return new Date(parsed).toISOString();
    }
  }

  return null;
}

function pickEventsArray(payload: unknown): unknown[] {
  if (Array.isArray(payload)) {
    return payload;
  }

  const root = asRecord(payload);
  if (!root) {
    return [];
  }

  const candidate = pickValue([root], ["events", "data", "results", "items"]);
  if (Array.isArray(candidate)) {
    return candidate;
  }

  return [];
}

function pickNextCursor(payload: unknown): string | null {
  const root = asRecord(payload);
  if (!root) {
    return null;
  }

  const pagination = asRecord(root.pagination) ?? asRecord(root.meta);
  const candidate = pickValue(
    [root, ...(pagination ? [pagination] : [])],
    ["next_cursor", "nextCursor", "cursor"],
  );
  return typeof candidate === "string" && candidate.trim().length > 0 ? candidate.trim() : null;
}

function normalizeSourceMode(raw: string): LeaderboardSourceMode | null {
  if (raw === "auto") {
    return "auto";
  }
  if (raw === "rpc") {
    return "rpc";
  }
  if (raw === "datalake") {
    return "datalake";
  }
  if (raw === "events_api") {
    return "events_api";
  }
  return null;
}

export function parseLeaderboardSourceMode(env: WorkerEnv): LeaderboardSourceMode {
  const raw = env.GALEXIE_SOURCE_MODE?.trim().toLowerCase();
  if (raw) {
    const normalized = normalizeSourceMode(raw);
    if (normalized) {
      return normalized;
    }
  }

  return "rpc";
}

function resolveRequestedSourceMode(
  options: GalexieFetchOptions,
): LeaderboardResolvedSourceMode | null {
  const raw = options.source?.trim().toLowerCase();
  if (!raw || raw === "default") {
    return null;
  }

  if (raw === "rpc" || raw === "events_api" || raw === "datalake") {
    return raw;
  }

  return null;
}

function parseLedgerCursor(cursor: string | null | undefined): number | null {
  if (!cursor || cursor.trim().length === 0) {
    return null;
  }

  const trimmed = cursor.trim();
  if (trimmed.startsWith("ledger:")) {
    const parsed = Number.parseInt(trimmed.slice("ledger:".length), 10);
    if (Number.isFinite(parsed) && parsed >= 0) {
      return parsed;
    }
    return null;
  }

  const parsed = Number.parseInt(trimmed, 10);
  if (Number.isFinite(parsed) && parsed >= 0) {
    return parsed;
  }

  return null;
}

function formatLedgerCursor(nextLedger: number): string {
  return `ledger:${Math.max(0, Math.trunc(nextLedger))}`;
}

function normalizeLedgerRange(
  options: GalexieFetchOptions,
  latestLedger: number | null,
): { fromLedger: number; toLedger: number } {
  const requestedFrom =
    typeof options.fromLedger === "number" && options.fromLedger >= 0
      ? Math.trunc(options.fromLedger)
      : parseLedgerCursor(options.cursor);
  const requestedTo =
    typeof options.toLedger === "number" && options.toLedger >= 0
      ? Math.trunc(options.toLedger)
      : null;

  const maxLedgers = Math.min(
    Math.max(options.limit ?? DEFAULT_GALEXIE_PAGE_LIMIT, 1),
    MAX_GALEXIE_PAGE_LIMIT,
  );

  const resolvedLatest = latestLedger !== null ? Math.max(2, latestLedger) : null;

  let fromLedger: number;
  if (requestedFrom !== null) {
    fromLedger = Math.max(2, requestedFrom);
  } else if (resolvedLatest !== null) {
    fromLedger = Math.max(2, resolvedLatest - DEFAULT_FORWARD_LEDGER_WINDOW + 1);
  } else {
    fromLedger = 2;
  }

  let toLedger: number;
  if (requestedTo !== null) {
    toLedger = Math.max(fromLedger, requestedTo);
  } else {
    toLedger = fromLedger + maxLedgers - 1;
    if (resolvedLatest !== null) {
      toLedger = Math.min(toLedger, resolvedLatest);
    }
  }

  return {
    fromLedger,
    toLedger,
  };
}

function asNativeMap(value: unknown): NativeMapLike | null {
  if (value instanceof Map) {
    return value;
  }

  if (value && typeof value === "object" && !Array.isArray(value)) {
    return value as Record<string, unknown>;
  }

  return null;
}

function readNativeMapValue(mapLike: NativeMapLike, keys: string[]): unknown {
  if (mapLike instanceof Map) {
    for (const key of keys) {
      if (mapLike.has(key)) {
        return mapLike.get(key);
      }
    }

    for (const [key, value] of mapLike.entries()) {
      if (typeof key === "string" && keys.includes(key)) {
        return value;
      }
    }

    return undefined;
  }

  for (const key of keys) {
    if (Object.prototype.hasOwnProperty.call(mapLike, key)) {
      return (mapLike as Record<string, unknown>)[key];
    }
  }

  return undefined;
}

function toHexString(value: unknown): string | null {
  if (typeof value === "string") {
    const trimmed = value.trim();
    return trimmed.length > 0 ? trimmed : null;
  }

  if (value instanceof Uint8Array) {
    return Array.from(value)
      .map((byte) => byte.toString(16).padStart(2, "0"))
      .join("");
  }

  if (value instanceof ArrayBuffer) {
    return toHexString(new Uint8Array(value));
  }

  return null;
}

function normalizeScoreSubmittedFromNative(nativeData: unknown): {
  claimantAddress: string;
  seed: number;
  previousBest: number;
  newBest: number;
  mintedDelta: number;
  journalDigest: string | null;
} | null {
  const mapLike = asNativeMap(nativeData);
  if (!mapLike) {
    return null;
  }

  const claimantRaw = readNativeMapValue(mapLike, [
    "claimant",
    "claimant_address",
    "claimantAddress",
  ]);
  if (typeof claimantRaw !== "string" || claimantRaw.trim().length === 0) {
    return null;
  }

  let claimantAddress: string;
  try {
    claimantAddress = validateClaimantStrKeyFromUserInput(claimantRaw);
  } catch {
    return null;
  }

  const seed = toInteger(readNativeMapValue(mapLike, ["seed"]));
  const newBest = toInteger(readNativeMapValue(mapLike, ["new_best", "newBest", "score"]));
  const previousBestRaw = toInteger(readNativeMapValue(mapLike, ["previous_best", "previousBest"]));
  const mintedDeltaRaw = toInteger(readNativeMapValue(mapLike, ["minted_delta", "mintedDelta"]));

  if (seed === null || seed < 0 || newBest === null || newBest <= 0) {
    return null;
  }

  const previousBest = previousBestRaw !== null && previousBestRaw >= 0 ? previousBestRaw : 0;
  const mintedDelta =
    mintedDeltaRaw !== null && mintedDeltaRaw >= 0
      ? mintedDeltaRaw
      : Math.max(0, newBest - previousBest);

  const journalDigest = toHexString(
    readNativeMapValue(mapLike, ["journal_digest", "journalDigest"]),
  );

  return {
    claimantAddress,
    seed: seed >>> 0,
    previousBest: previousBest >>> 0,
    newBest: newBest >>> 0,
    mintedDelta: mintedDelta >>> 0,
    journalDigest,
  };
}

function getLedgerMetaVersionBody(
  ledgerMeta: xdr.LedgerCloseMeta,
): xdr.LedgerCloseMetaV2 | xdr.LedgerCloseMetaV1 | xdr.LedgerCloseMetaV0 | null {
  try {
    return ledgerMeta.v2();
  } catch {
    // ignore
  }
  try {
    return ledgerMeta.v1();
  } catch {
    // ignore
  }
  try {
    return ledgerMeta.v0();
  } catch {
    // ignore
  }
  return null;
}

function getTxApplyProcessingV4(txApplyProcessing: unknown): { events?: () => unknown[] } | null {
  for (const getter of ["v4", "v3", "v2", "v1"] as const) {
    try {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      return (txApplyProcessing as any)[getter]();
    } catch {
      // ignore
    }
  }

  return null;
}

function scValToNativeSafe(value: xdr.ScVal): unknown {
  try {
    return scValToNative(value);
  } catch {
    return null;
  }
}

function normalizeEventName(value: unknown): string | null {
  if (typeof value !== "string") {
    return null;
  }

  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

function normalizeEventKey(value: unknown): string | null {
  if (typeof value !== "string") {
    return null;
  }

  const normalized = value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9_]/g, "");
  return normalized.length > 0 ? normalized : null;
}

function decodeScValBase64(value: unknown): unknown {
  if (typeof value !== "string" || value.trim().length === 0) {
    return null;
  }

  try {
    return scValToNativeSafe(xdr.ScVal.fromXDR(value.trim(), "base64"));
  } catch {
    return null;
  }
}

function extractScoreEventsFromLedgerBatch(
  compressedBody: Uint8Array,
  scoreContractId: string | null,
  ledgerRange: { fromLedger: number; toLedger: number },
  ingestedAt: string,
): { events: LeaderboardEventRecord[]; inspectedEventCount: number } {
  const decompressed = decompress(compressedBody);
  const batch = xdr.LedgerCloseMetaBatch.fromXDR(decompressed as unknown as Buffer);
  const ledgerMetas = batch.ledgerCloseMeta();

  const events: LeaderboardEventRecord[] = [];
  let inspectedEventCount = 0;

  for (let metaIndex = 0; metaIndex < ledgerMetas.length; metaIndex += 1) {
    const meta = ledgerMetas[metaIndex];
    const body = getLedgerMetaVersionBody(meta);
    if (!body) {
      continue;
    }

    const ledgerHeader = body.ledgerHeader().header();
    const ledgerSeq = Math.trunc(ledgerHeader.ledgerSeq());
    if (ledgerSeq < ledgerRange.fromLedger || ledgerSeq > ledgerRange.toLedger) {
      continue;
    }

    const closedAt = toIsoTimestamp(ledgerHeader.scpValue().closeTime()) ?? nowIso();

    const txProcessing = body.txProcessing();
    for (let txIndex = 0; txIndex < txProcessing.length; txIndex += 1) {
      const tx = txProcessing[txIndex];
      const txHash = toHexString(tx.result().transactionHash());

      const txApplyV4 = getTxApplyProcessingV4(tx.txApplyProcessing());
      if (!txApplyV4 || typeof txApplyV4.events !== "function") {
        continue;
      }

      const txEvents = txApplyV4.events();
      for (let eventIndex = 0; eventIndex < txEvents.length; eventIndex += 1) {
        inspectedEventCount += 1;

        const wrappedEvent = txEvents[eventIndex] as { event: () => xdr.ContractEvent };
        const contractEvent = wrappedEvent.event();
        if (contractEvent.type().name !== "contract") {
          continue;
        }

        let eventContractId: string;
        try {
          eventContractId = StrKey.encodeContract(contractEvent.contractId() as unknown as Buffer);
        } catch {
          continue;
        }
        if (scoreContractId && eventContractId !== scoreContractId) {
          continue;
        }

        let eventBody: xdr.ContractEventV0;
        try {
          eventBody = contractEvent.body().v0();
        } catch {
          continue;
        }

        const nativeTopics = eventBody.topics().map((topic) => scValToNativeSafe(topic));
        const eventName = normalizeEventName(nativeTopics[0]);
        if (!eventName || !SCORE_EVENT_NAMES.has(eventName)) {
          continue;
        }

        const scoreEvent = normalizeScoreSubmittedFromNative(scValToNativeSafe(eventBody.data()));
        if (!scoreEvent) {
          continue;
        }

        const eventId = `${txHash ?? "tx"}:${ledgerSeq}:${txIndex}:${eventIndex}`;

        events.push({
          eventId,
          claimantAddress: scoreEvent.claimantAddress,
          seed: scoreEvent.seed,
          previousBest: scoreEvent.previousBest,
          newBest: scoreEvent.newBest,
          mintedDelta: scoreEvent.mintedDelta,
          journalDigest: scoreEvent.journalDigest,
          txHash,
          eventIndex,
          ledger: ledgerSeq,
          closedAt,
          source: "galexie",
          ingestedAt,
        });
      }
    }
  }

  return {
    events,
    inspectedEventCount,
  };
}

function getGalexieRootPath(env: WorkerEnv): string {
  const raw = env.GALEXIE_DATASTORE_ROOT_PATH?.trim() ?? DEFAULT_GALEXIE_DATA_ROOT_PATH;
  if (raw.length === 0) {
    return DEFAULT_GALEXIE_DATA_ROOT_PATH;
  }

  return raw.startsWith("/") ? raw : `/${raw}`;
}

function getGalexieObjectExtension(env: WorkerEnv): string {
  const raw = env.GALEXIE_DATASTORE_OBJECT_EXTENSION?.trim();
  if (!raw || raw.length === 0) {
    return DEFAULT_GALEXIE_OBJECT_EXTENSION;
  }

  return raw.replace(/^\.+/, "");
}

function normalizeCompressionExtension(raw: string): string {
  const normalized = raw.trim().toLowerCase().replace(/^\.+/, "");
  if (normalized === "zstd") {
    return "zstd";
  }
  if (normalized === "zst") {
    return "zst";
  }
  return normalized;
}

function resolveGalexieDatastoreObjectExtensions(
  env: WorkerEnv,
  datastoreConfig: GalexieDatastoreConfig,
): string[] {
  const candidates: string[] = [];
  const configured = env.GALEXIE_DATASTORE_OBJECT_EXTENSION?.trim();
  if (configured && configured.length > 0) {
    candidates.push(configured);
  }

  const compression = datastoreConfig.compression?.trim();
  if (compression && compression.length > 0) {
    candidates.push(compression);
    if (compression.trim().toLowerCase() === "zstd") {
      candidates.push("zst");
    } else if (compression.trim().toLowerCase() === "zst") {
      candidates.push("zstd");
    }
  }

  candidates.push(getGalexieObjectExtension(env), "zst", "zstd");

  const unique = new Set<string>();
  for (const candidate of candidates) {
    const normalized = normalizeCompressionExtension(candidate);
    if (normalized.length > 0) {
      unique.add(normalized);
    }
  }

  return Array.from(unique);
}

function getGalexieDatastoreObjectKey(
  ledgerSeq: number,
  datastoreConfig: GalexieDatastoreConfig,
  extension: string,
): string {
  const maxUint32 = 0xffff_ffff;
  const ledgersPerFile = Math.max(1, Math.trunc(datastoreConfig.ledgersPerBatch));
  const filesPerPartition = Math.max(1, Math.trunc(datastoreConfig.batchesPerPartition));

  const partitionSize = ledgersPerFile * filesPerPartition;
  const partitionStart = Math.floor(ledgerSeq / partitionSize) * partitionSize;
  const partitionEnd = Math.min(maxUint32, partitionStart + partitionSize - 1);

  const fileStart = Math.floor(ledgerSeq / ledgersPerFile) * ledgersPerFile;
  const fileEnd = Math.min(maxUint32, fileStart + ledgersPerFile - 1);

  let objectKey = "";
  if (filesPerPartition > 1) {
    const reversedPartition = (maxUint32 - partitionStart)
      .toString(16)
      .toUpperCase()
      .padStart(8, "0");
    objectKey += `${reversedPartition}--${partitionStart}-${partitionEnd}/`;
  }

  const reversedFile = (maxUint32 - fileStart).toString(16).toUpperCase().padStart(8, "0");
  objectKey += `${reversedFile}--${fileStart}`;
  if (fileEnd !== fileStart) {
    objectKey += `-${fileEnd}`;
  }

  return `${objectKey}.xdr.${extension}`;
}

function isGalexieConfigured(env: WorkerEnv): boolean {
  return Boolean(env.GALEXIE_API_BASE_URL?.trim());
}

function validateGalexieBaseUrl(env: WorkerEnv): URL {
  const raw = env.GALEXIE_API_BASE_URL?.trim();
  if (!raw) {
    throw new Error("GALEXIE_API_BASE_URL is not configured");
  }

  let base: URL;
  try {
    base = new URL(raw);
  } catch {
    throw new Error("GALEXIE_API_BASE_URL is invalid");
  }

  if (base.protocol !== "https:" && base.protocol !== "http:") {
    throw new Error("GALEXIE_API_BASE_URL must use http or https");
  }

  return base;
}

function getRpcBaseUrl(env: WorkerEnv): URL {
  const raw = env.GALEXIE_RPC_BASE_URL?.trim() ?? DEFAULT_RPC_BASE_URL;

  let base: URL;
  try {
    base = new URL(raw);
  } catch {
    throw new Error("GALEXIE_RPC_BASE_URL is invalid");
  }

  if (base.protocol !== "https:" && base.protocol !== "http:") {
    throw new Error("GALEXIE_RPC_BASE_URL must use http or https");
  }

  return base;
}

async function fetchWithTimeout(url: URL, init: RequestInit, timeoutMs: number): Promise<Response> {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), timeoutMs);
  try {
    return await fetch(url, {
      ...init,
      signal: controller.signal,
    });
  } catch (error) {
    if (error instanceof DOMException && error.name === "AbortError") {
      throw new Error(`leaderboard source request timed out after ${timeoutMs}ms`, {
        cause: error,
      });
    }
    throw error;
  } finally {
    clearTimeout(timeout);
  }
}

function getGalexieAuthHeaders(env: WorkerEnv): Record<string, string> {
  const headers: Record<string, string> = {
    accept: "application/json",
  };

  if (env.GALEXIE_API_KEY && env.GALEXIE_API_KEY.trim().length > 0) {
    const apiKey = env.GALEXIE_API_KEY.trim();
    headers.authorization = `Bearer ${apiKey}`;
    headers["x-api-key"] = apiKey;
  }

  return headers;
}

async function fetchRpcLatestLedger(env: WorkerEnv, timeoutMs: number): Promise<number | null> {
  const rpcBase = getRpcBaseUrl(env);
  const body = {
    jsonrpc: "2.0",
    id: 1,
    method: "getHealth",
  };

  const response = await fetchWithTimeout(
    rpcBase,
    {
      method: "POST",
      headers: {
        ...getGalexieAuthHeaders(env),
        "content-type": "application/json",
      },
      body: JSON.stringify(body),
    },
    timeoutMs,
  );

  if (!response.ok) {
    return null;
  }

  try {
    const payload = (await response.json()) as JsonRecord;
    const result = asRecord(payload.result);
    const latestLedger = toInteger(result?.latestLedger);
    if (latestLedger !== null && latestLedger >= 0) {
      return latestLedger;
    }
  } catch {
    // ignore parse errors and fallback to null
  }

  return null;
}

async function fetchGalexieDatastoreConfig(
  env: WorkerEnv,
  baseUrl: URL,
  timeoutMs: number,
): Promise<GalexieDatastoreConfig> {
  const rootPath = getGalexieRootPath(env);
  const configUrl = new URL(`${rootPath}/.config.json`, baseUrl);

  const response = await fetchWithTimeout(
    configUrl,
    {
      method: "GET",
      headers: getGalexieAuthHeaders(env),
    },
    timeoutMs,
  );

  if (!response.ok) {
    throw new Error(`galexie datastore config request failed (${response.status})`);
  }

  const payload = (await response.json()) as JsonRecord;
  const schema = asRecord(payload.schema);
  const nested = [payload, ...(schema ? [schema] : [])];
  const ledgersPerBatch = toInteger(
    pickValue(nested, ["ledgersPerBatch", "ledgersPerFile", "ledgers_per_file"]),
  );
  const batchesPerPartition = toInteger(
    pickValue(nested, ["batchesPerPartition", "filesPerPartition", "files_per_partition"]),
  );
  const compressionRaw = pickValue(nested, ["compression", "file_extension", "fileExtension"]);
  const networkPassphraseRaw = pickValue(nested, ["networkPassphrase", "network_passphrase"]);

  if (!ledgersPerBatch || ledgersPerBatch <= 0) {
    throw new Error("galexie datastore config missing ledgersPerBatch");
  }
  if (!batchesPerPartition || batchesPerPartition <= 0) {
    throw new Error("galexie datastore config missing batchesPerPartition");
  }

  return {
    networkPassphrase: typeof networkPassphraseRaw === "string" ? networkPassphraseRaw : "unknown",
    ledgersPerBatch,
    batchesPerPartition,
    compression:
      typeof compressionRaw === "string"
        ? normalizeCompressionExtension(compressionRaw)
        : DEFAULT_GALEXIE_OBJECT_EXTENSION,
  };
}

function normalizeScoreContractId(env: WorkerEnv): string | null {
  const raw = env.SCORE_CONTRACT_ID?.trim();
  if (!raw) {
    return null;
  }

  try {
    return StrKey.encodeContract(StrKey.decodeContract(raw));
  } catch {
    return raw;
  }
}

function normalizeRpcGetEventsPayload(payload: unknown, ingestedAt = nowIso()): GalexieFetchResult {
  const root = asRecord(payload);
  const rawEvents = Array.isArray(root?.events) ? root.events : [];
  const events: LeaderboardEventRecord[] = [];

  for (const rawEvent of rawEvents) {
    const eventRecord = asRecord(rawEvent);
    if (!eventRecord) {
      continue;
    }

    const nested = [eventRecord, asRecord(eventRecord.event)].filter(
      (value): value is JsonRecord => value !== null,
    );
    const rawTopics = pickValue(nested, ["topic", "topics"]);
    const topic =
      Array.isArray(rawTopics) && rawTopics.length > 0 ? decodeScValBase64(rawTopics[0]) : null;
    const eventKey = normalizeEventKey(topic);
    if (!eventKey || !SCORE_EVENT_KEYS.has(eventKey)) {
      continue;
    }

    const scoreEvent = normalizeScoreSubmittedFromNative(
      decodeScValBase64(pickValue(nested, ["value", "data"])),
    );
    if (!scoreEvent) {
      continue;
    }

    const txHashRaw = pickValue(nested, [
      "txHash",
      "tx_hash",
      "transactionHash",
      "transaction_hash",
    ]);
    const txHash =
      typeof txHashRaw === "string" && txHashRaw.trim().length > 0 ? txHashRaw.trim() : null;
    const eventIndexRaw = toInteger(
      pickValue(nested, ["eventIndex", "event_index", "operationIndex", "opIndex", "index"]),
    );
    const eventIndex = eventIndexRaw !== null && eventIndexRaw >= 0 ? eventIndexRaw : null;
    const ledgerRaw = toInteger(pickValue(nested, ["ledger", "ledger_sequence", "ledgerSequence"]));
    const ledger = ledgerRaw !== null && ledgerRaw >= 0 ? ledgerRaw : null;
    const closedAt = toIsoTimestamp(
      pickValue(nested, [
        "ledgerClosedAt",
        "ledger_closed_at",
        "closedAt",
        "closed_at",
        "timestamp",
      ]),
    );
    if (!closedAt) {
      continue;
    }

    const explicitEventId = pickValue(nested, [
      "id",
      "event_id",
      "eventId",
      "pagingToken",
      "cursor",
    ]);
    let eventId =
      typeof explicitEventId === "string" && explicitEventId.trim().length > 0
        ? explicitEventId.trim()
        : null;
    if (!eventId && txHash) {
      eventId = `${txHash}:${eventIndex ?? 0}`;
    }
    if (!eventId && ledger !== null) {
      eventId = `${ledger}:${eventIndex ?? 0}`;
    }
    if (!eventId) {
      continue;
    }

    events.push({
      eventId,
      claimantAddress: scoreEvent.claimantAddress,
      seed: scoreEvent.seed,
      previousBest: scoreEvent.previousBest,
      newBest: scoreEvent.newBest,
      mintedDelta: scoreEvent.mintedDelta,
      journalDigest: scoreEvent.journalDigest,
      txHash,
      eventIndex,
      ledger,
      closedAt,
      source: "rpc",
      ingestedAt,
    });
  }

  const cursorRaw = pickValue([root ?? {}], ["cursor", "next_cursor", "nextCursor"]);
  const nextCursor =
    typeof cursorRaw === "string" && cursorRaw.trim().length > 0 ? cursorRaw.trim() : null;

  return {
    events,
    nextCursor,
    fetchedCount: rawEvents.length,
    provider: "rpc",
    sourceMode: "rpc",
  };
}

async function fetchLeaderboardEventsFromRpcEvents(
  env: WorkerEnv,
  options: GalexieFetchOptions,
): Promise<GalexieFetchResult> {
  const timeoutMs = parseInteger(env.GALEXIE_REQUEST_TIMEOUT_MS, DEFAULT_GALEXIE_TIMEOUT_MS, 1_000);
  const rpcBase = getRpcBaseUrl(env);
  const limit = Math.min(
    Math.max(options.limit ?? DEFAULT_GALEXIE_PAGE_LIMIT, 1),
    MAX_GALEXIE_PAGE_LIMIT,
  );
  const scoreContractId = normalizeScoreContractId(env);

  const pagination: JsonRecord = {
    limit,
  };
  if (options.cursor && options.cursor.trim().length > 0) {
    pagination.cursor = options.cursor.trim();
  }

  const filter: JsonRecord = {
    type: "contract",
  };
  if (scoreContractId) {
    filter.contractIds = [scoreContractId];
  }

  const params: JsonRecord = {
    filters: [filter],
    pagination,
  };

  if (typeof options.fromLedger === "number" && options.fromLedger >= 2) {
    params.startLedger = Math.trunc(options.fromLedger);
  }
  if (typeof options.toLedger === "number" && options.toLedger >= 2) {
    params.endLedger = Math.trunc(options.toLedger);
  }

  const response = await fetchWithTimeout(
    rpcBase,
    {
      method: "POST",
      headers: {
        ...getGalexieAuthHeaders(env),
        "content-type": "application/json",
      },
      body: JSON.stringify({
        jsonrpc: "2.0",
        id: 1,
        method: "getEvents",
        params,
      }),
    },
    timeoutMs,
  );

  if (!response.ok) {
    const detail = (await response.text()).slice(0, 300);
    throw new Error(
      detail.length > 0
        ? `rpc getEvents request failed (${response.status}): ${detail}`
        : `rpc getEvents request failed (${response.status})`,
    );
  }

  let payload: unknown;
  try {
    payload = await response.json();
  } catch (error) {
    throw new Error(`rpc getEvents response was not valid JSON: ${safeErrorMessage(error)}`, {
      cause: error,
    });
  }

  const root = asRecord(payload);
  const errorPayload = asRecord(root?.error);
  if (errorPayload) {
    const code = toInteger(errorPayload.code);
    const message = typeof errorPayload.message === "string" ? errorPayload.message.trim() : null;
    const details = typeof errorPayload.data === "string" ? errorPayload.data.trim() : null;
    const pieces = [
      "rpc getEvents returned an error",
      code !== null ? `code=${code}` : null,
      message ? `message=${message}` : null,
      details ? `data=${details}` : null,
    ].filter((value): value is string => value !== null);
    throw new Error(pieces.join(", "));
  }

  const result = asRecord(root?.result);
  if (!result) {
    throw new Error("rpc getEvents response missing result");
  }

  return normalizeRpcGetEventsPayload(result);
}

async function fetchLeaderboardEventsFromGalexieDatastore(
  env: WorkerEnv,
  options: GalexieFetchOptions,
): Promise<GalexieFetchResult> {
  const timeoutMs = parseInteger(env.GALEXIE_REQUEST_TIMEOUT_MS, DEFAULT_GALEXIE_TIMEOUT_MS, 1_000);
  const baseUrl = validateGalexieBaseUrl(env);
  const datastoreConfig = await fetchGalexieDatastoreConfig(env, baseUrl, timeoutMs);
  const latestLedger = await fetchRpcLatestLedger(env, timeoutMs);

  const ledgerRange = normalizeLedgerRange(options, latestLedger);
  const scoreContractId = normalizeScoreContractId(env);
  const extensions = resolveGalexieDatastoreObjectExtensions(env, datastoreConfig);
  const rootPath = getGalexieRootPath(env);

  const events: LeaderboardEventRecord[] = [];
  let inspectedEventCount = 0;
  let cursorLedger = ledgerRange.fromLedger;
  let consecutiveMissingFiles = 0;

  const ledgersPerFile = Math.max(1, datastoreConfig.ledgersPerBatch);
  let fileStartLedger = Math.floor(ledgerRange.fromLedger / ledgersPerFile) * ledgersPerFile;

  // Intentionally sequential: we stream contiguous files and stop quickly on trailing 404s.
  // eslint-disable-next-line no-await-in-loop
  while (fileStartLedger <= ledgerRange.toLedger) {
    let resolvedResponse: Response | null = null;
    for (const extension of extensions) {
      const key = getGalexieDatastoreObjectKey(fileStartLedger, datastoreConfig, extension);
      const objectUrl = new URL(`${rootPath}/${key}`, baseUrl);
      // eslint-disable-next-line no-await-in-loop
      const response = await fetchWithTimeout(
        objectUrl,
        {
          method: "GET",
          headers: {
            ...getGalexieAuthHeaders(env),
            accept: "application/octet-stream",
          },
        },
        timeoutMs,
      );

      if (response.status === 404) {
        continue;
      }
      if (!response.ok) {
        throw new Error(`galexie datastore object request failed (${response.status})`);
      }

      resolvedResponse = response;
      break;
    }

    if (!resolvedResponse) {
      consecutiveMissingFiles += 1;
      fileStartLedger += ledgersPerFile;
      cursorLedger = fileStartLedger;
      if (options.toLedger === null || options.toLedger === undefined) {
        if (consecutiveMissingFiles >= 2) {
          break;
        }
      }
      continue;
    }

    consecutiveMissingFiles = 0;

    // eslint-disable-next-line no-await-in-loop
    const compressedBody = new Uint8Array(await resolvedResponse.arrayBuffer());
    const extracted = extractScoreEventsFromLedgerBatch(
      compressedBody,
      scoreContractId,
      ledgerRange,
      nowIso(),
    );

    events.push(...extracted.events);
    inspectedEventCount += extracted.inspectedEventCount;

    fileStartLedger += ledgersPerFile;
    cursorLedger = fileStartLedger;
  }

  return {
    events,
    nextCursor: formatLedgerCursor(cursorLedger),
    fetchedCount: inspectedEventCount,
    provider: "galexie",
    sourceMode: "datalake",
  };
}

export function normalizeGalexieScoreEvents(
  payload: unknown,
  ingestedAt = nowIso(),
): GalexieFetchResult {
  const rawEvents = pickEventsArray(payload);
  const events: LeaderboardEventRecord[] = [];

  for (const rawEvent of rawEvents) {
    const root = asRecord(rawEvent);
    if (!root) {
      continue;
    }

    const nested = [
      root,
      asRecord(root.data),
      asRecord(root.payload),
      asRecord(root.attributes),
      asRecord(root.value),
      asRecord(root.event),
    ].filter((value): value is JsonRecord => value !== null);

    const claimantRaw = pickValue(nested, ["claimant", "claimant_address", "claimantAddress"]);
    const claimantString =
      typeof claimantRaw === "string"
        ? claimantRaw
        : (() => {
            const claimantObj = asRecord(claimantRaw);
            if (!claimantObj) {
              return null;
            }
            const nestedAddress = pickValue(
              [claimantObj],
              ["address", "value", "id", "contract_id"],
            );
            return typeof nestedAddress === "string" ? nestedAddress : null;
          })();
    if (!claimantString) {
      continue;
    }

    let claimantAddress: string;
    try {
      claimantAddress = validateClaimantStrKeyFromUserInput(claimantString);
    } catch {
      continue;
    }

    const seed = toInteger(pickValue(nested, ["seed"]));
    const newBest = toInteger(pickValue(nested, ["new_best", "newBest", "score", "final_score"]));
    if (seed === null || seed < 0 || newBest === null || newBest <= 0) {
      continue;
    }

    const previousBestRaw = toInteger(pickValue(nested, ["previous_best", "previousBest"]));
    const previousBest = previousBestRaw !== null && previousBestRaw >= 0 ? previousBestRaw : 0;
    const mintedDeltaRaw = toInteger(pickValue(nested, ["minted_delta", "mintedDelta"]));
    const mintedDelta =
      mintedDeltaRaw !== null && mintedDeltaRaw >= 0
        ? mintedDeltaRaw
        : Math.max(0, newBest - previousBest);

    const journalDigestRaw = pickValue(nested, ["journal_digest", "journalDigest", "digest"]);
    const journalDigest =
      typeof journalDigestRaw === "string" && journalDigestRaw.trim().length > 0
        ? journalDigestRaw.trim()
        : null;

    const txHashRaw = pickValue(nested, [
      "tx_hash",
      "txHash",
      "transaction_hash",
      "transactionHash",
    ]);
    const txHash =
      typeof txHashRaw === "string" && txHashRaw.trim().length > 0 ? txHashRaw.trim() : null;

    const eventIndexRaw = toInteger(
      pickValue(nested, ["event_index", "eventIndex", "index", "log_index"]),
    );
    const eventIndex = eventIndexRaw !== null && eventIndexRaw >= 0 ? eventIndexRaw : null;
    const ledgerRaw = toInteger(pickValue(nested, ["ledger", "ledger_sequence", "ledgerSequence"]));
    const ledger = ledgerRaw !== null && ledgerRaw >= 0 ? ledgerRaw : null;

    const closedAt = toIsoTimestamp(
      pickValue(nested, [
        "closed_at",
        "closedAt",
        "ledger_closed_at",
        "ledgerClosedAt",
        "timestamp",
        "created_at",
        "createdAt",
      ]),
    );
    if (!closedAt) {
      continue;
    }

    const explicitEventId = pickValue(nested, ["event_id", "eventId", "id", "cursor"]);
    let eventId =
      typeof explicitEventId === "string" && explicitEventId.trim().length > 0
        ? explicitEventId.trim()
        : null;

    if (!eventId && txHash) {
      eventId = `${txHash}:${eventIndex ?? 0}`;
    }
    if (!eventId && ledger !== null) {
      eventId = `${ledger}:${eventIndex ?? 0}`;
    }
    if (!eventId) {
      continue;
    }

    events.push({
      eventId,
      claimantAddress,
      seed: seed >>> 0,
      previousBest: previousBest >>> 0,
      newBest: newBest >>> 0,
      mintedDelta: mintedDelta >>> 0,
      journalDigest,
      txHash,
      eventIndex,
      ledger,
      closedAt,
      source: "galexie",
      ingestedAt,
    });
  }

  return {
    events,
    nextCursor: pickNextCursor(payload),
    fetchedCount: rawEvents.length,
    provider: "galexie",
    sourceMode: "events_api",
  };
}

async function fetchLeaderboardEventsFromGalexieEventsApi(
  env: WorkerEnv,
  options: GalexieFetchOptions,
): Promise<GalexieFetchResult> {
  const base = validateGalexieBaseUrl(env);
  const endpointPath = env.GALEXIE_SCORE_EVENTS_PATH?.trim() || DEFAULT_GALEXIE_EVENTS_PATH;
  const timeoutMs = parseInteger(env.GALEXIE_REQUEST_TIMEOUT_MS, DEFAULT_GALEXIE_TIMEOUT_MS, 1_000);

  const url = new URL(endpointPath, base);
  const limit = Math.min(
    Math.max(options.limit ?? DEFAULT_GALEXIE_PAGE_LIMIT, 1),
    MAX_GALEXIE_PAGE_LIMIT,
  );
  url.searchParams.set("limit", `${limit}`);
  url.searchParams.set("order", "asc");
  url.searchParams.set("event_name", "ScoreSubmitted");

  const scoreContractId = env.SCORE_CONTRACT_ID?.trim();
  if (scoreContractId) {
    url.searchParams.set("contract_id", scoreContractId);
  }

  if (options.cursor && options.cursor.trim().length > 0) {
    url.searchParams.set("cursor", options.cursor.trim());
  }
  if (typeof options.fromLedger === "number" && options.fromLedger >= 0) {
    url.searchParams.set("from_ledger", `${Math.trunc(options.fromLedger)}`);
  }
  if (typeof options.toLedger === "number" && options.toLedger >= 0) {
    url.searchParams.set("to_ledger", `${Math.trunc(options.toLedger)}`);
  }

  const response = await fetchWithTimeout(
    url,
    {
      method: "GET",
      headers: getGalexieAuthHeaders(env),
    },
    timeoutMs,
  );

  if (!response.ok) {
    let detail = `galexie request failed (${response.status})`;
    try {
      const payload = await response.json();
      const message = asRecord(payload)?.error;
      if (typeof message === "string" && message.trim().length > 0) {
        detail = `${detail}: ${message.trim()}`;
      }
    } catch {
      // ignore parse failures and use fallback detail
    }
    throw new Error(detail);
  }

  let payload: unknown;
  try {
    payload = await response.json();
  } catch (error) {
    throw new Error(`galexie response was not valid JSON: ${safeErrorMessage(error)}`, {
      cause: error,
    });
  }

  return normalizeGalexieScoreEvents(payload);
}

export async function fetchLeaderboardEventsFromGalexie(
  env: WorkerEnv,
  options: GalexieFetchOptions,
): Promise<GalexieFetchResult> {
  const configuredMode = parseLeaderboardSourceMode(env);
  const overrideMode = resolveRequestedSourceMode(options);
  const effectiveMode = overrideMode ?? configuredMode;

  const tryMode = async (mode: LeaderboardResolvedSourceMode): Promise<GalexieFetchResult> => {
    if (mode === "rpc") {
      return fetchLeaderboardEventsFromRpcEvents(env, options);
    }
    if (mode === "datalake") {
      return fetchLeaderboardEventsFromGalexieDatastore(env, options);
    }
    return fetchLeaderboardEventsFromGalexieEventsApi(env, options);
  };

  const fallbackModes: LeaderboardResolvedSourceMode[] = (() => {
    if (effectiveMode === "rpc") {
      return isGalexieConfigured(env) ? ["rpc", "datalake", "events_api"] : ["rpc"];
    }
    if (effectiveMode === "auto") {
      return isGalexieConfigured(env) ? ["rpc", "datalake", "events_api"] : ["rpc"];
    }
    if (effectiveMode === "datalake") {
      return ["datalake", "rpc"];
    }
    return ["events_api", "rpc"];
  })();

  const errors: string[] = [];
  // Intentionally sequential so fallbacks execute in strict priority order.
  for (const mode of fallbackModes) {
    try {
      // eslint-disable-next-line no-await-in-loop
      return await tryMode(mode);
    } catch (error) {
      errors.push(`${mode}: ${safeErrorMessage(error)}`);
    }
  }

  throw new Error(`all leaderboard ingestion sources failed (${errors.join(" | ")})`);
}
