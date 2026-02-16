import { MAX_RETRY_DELAY_SECONDS } from "./constants";
import type { ProofJobStatus } from "./types";

export function nowIso(): string {
  return new Date().toISOString();
}

export function parseInteger(raw: string | undefined, fallback: number, minimum = 1): number {
  if (raw === undefined) {
    return fallback;
  }

  const parsed = Number.parseInt(raw, 10);
  if (!Number.isFinite(parsed) || parsed < minimum) {
    return fallback;
  }

  return parsed;
}

export function parseBoolean(raw: string | undefined, fallback: boolean): boolean {
  if (raw === undefined) {
    return fallback;
  }

  const normalized = raw.trim().toLowerCase();
  if (normalized === "1" || normalized === "true" || normalized === "yes" || normalized === "on") {
    return true;
  }

  if (normalized === "0" || normalized === "false" || normalized === "no" || normalized === "off") {
    return false;
  }

  return fallback;
}

export function safeErrorMessage(error: unknown): string {
  const raw =
    error instanceof Error && error.message && error.message.trim().length > 0
      ? error.message
      : String(error);
  // Collapse control chars (including embedded binary bytes) so responses
  // remain valid JSON and readable in UI surfaces.
  return raw.replace(/[\u0000-\u001f\u007f]+/g, " ").trim();
}

export function isLocalHostname(hostname: string): boolean {
  return hostname === "localhost" || hostname === "127.0.0.1" || hostname === "::1";
}

export function isTerminalProofStatus(status: ProofJobStatus): boolean {
  return status === "succeeded" || status === "failed";
}

export function retryDelaySeconds(attempt: number): number {
  const base = Math.min(2 ** Math.min(Math.max(attempt - 1, 0), 7), MAX_RETRY_DELAY_SECONDS);
  return Math.max(2, base);
}

export function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => {
    setTimeout(resolve, ms);
  });
}
