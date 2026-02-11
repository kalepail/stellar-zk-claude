import { useCallback, useEffect, useState } from "react";
import { AsteroidsCanvas, type CompletedGameRun } from "./components/AsteroidsCanvas";
import {
  cancelProofJob,
  getGatewayHealth,
  ProofApiError,
  type GatewayHealthResponse,
  getProofJob,
  isTerminalProofStatus,
  submitProofJob,
  type ProofJobPublic,
  type ProofJobStatus,
} from "./proof/api";
import { deserializeTape } from "./game/tape";
import type {
  SmartAccountConfig,
  SmartAccountRelayerMode,
  SmartWalletSession,
} from "./wallet/smartAccount";
import "./App.css";

function formatHex32(value: number): string {
  return `0x${(value >>> 0).toString(16).toUpperCase().padStart(8, "0")}`;
}

function abbreviateHex(value: string, keep = 8): string {
  if (value.length <= keep * 2) {
    return value;
  }
  return `${value.slice(0, keep)}...${value.slice(-keep)}`;
}

function formatDuration(ms: number): string {
  if (!Number.isFinite(ms) || ms <= 0) {
    return "0 ms";
  }

  if (ms < 1000) {
    return `${ms} ms`;
  }

  const seconds = ms / 1000;
  if (seconds < 60) {
    return `${seconds.toFixed(1)} s`;
  }

  const minutes = Math.floor(seconds / 60);
  const leftoverSeconds = Math.round(seconds % 60);
  return `${minutes}m ${leftoverSeconds}s`;
}

function statusLabel(status: ProofJobStatus): string {
  switch (status) {
    case "queued":
      return "Queued";
    case "dispatching":
      return "Dispatching";
    case "prover_running":
      return "Running";
    case "retrying":
      return "Retrying";
    case "succeeded":
      return "Succeeded";
    case "failed":
      return "Failed";
    default:
      return status;
  }
}

function statusClassName(status: ProofJobStatus | "idle"): string {
  return `status-chip status-chip--${status}`;
}

type WalletAction = "idle" | "restoring" | "connecting" | "creating" | "disconnecting";

function walletActionLabel(action: WalletAction, connected: boolean): string {
  if (action === "idle") {
    return connected ? "Connected" : "Not Connected";
  }

  switch (action) {
    case "restoring":
      return "Restoring";
    case "connecting":
      return "Connecting";
    case "creating":
      return "Creating";
    case "disconnecting":
      return "Disconnecting";
    default:
      return "Wallet";
  }
}

function walletChipClassName(action: WalletAction, connected: boolean): string {
  if (action !== "idle") {
    return "wallet-chip wallet-chip--busy";
  }

  return connected ? "wallet-chip wallet-chip--connected" : "wallet-chip wallet-chip--disconnected";
}

function relayerModeLabel(mode: SmartAccountRelayerMode): string {
  switch (mode) {
    case "channels-api-key":
      return "OpenZeppelin Channels (API Key)";
    case "channels-missing-key":
      return "Channels URL Set (Missing API Key)";
    case "proxy":
      return "Relayer Proxy";
    default:
      return "Not Configured";
  }
}

type SmartWalletModule = typeof import("./wallet/smartAccount");

let smartWalletModulePromise: Promise<SmartWalletModule> | null = null;

async function loadSmartWalletModule(): Promise<SmartWalletModule> {
  if (!smartWalletModulePromise) {
    smartWalletModulePromise = import("./wallet/smartAccount");
  }

  return smartWalletModulePromise;
}

function App() {
  const [latestRun, setLatestRun] = useState<CompletedGameRun | null>(null);
  const [proofJob, setProofJob] = useState<ProofJobPublic | null>(null);
  const [proofError, setProofError] = useState<string | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [walletSession, setWalletSession] = useState<SmartWalletSession | null>(null);
  const [walletAction, setWalletAction] = useState<WalletAction>("idle");
  const [walletUserName, setWalletUserName] = useState("");
  const [walletError, setWalletError] = useState<string | null>(null);
  const [walletConfig, setWalletConfig] = useState<Pick<SmartAccountConfig, "networkPassphrase">>({
    networkPassphrase: "Test SDF Network ; September 2015",
  });
  const [walletRelayerMode, setWalletRelayerMode] = useState<SmartAccountRelayerMode>("disabled");
  const [gatewayHealth, setGatewayHealth] = useState<GatewayHealthResponse | null>(null);
  const [gatewayHealthError, setGatewayHealthError] = useState<string | null>(null);
  const activeProofJobId = proofJob?.jobId ?? null;
  const activeProofJobStatus = proofJob?.status ?? null;
  const claimantAddress = walletSession?.contractId ?? "";

  const handleGameOver = useCallback((run: CompletedGameRun) => {
    setLatestRun(run);
    setProofError(null);
    setProofJob((current) => {
      if (!current) {
        return null;
      }

      return isTerminalProofStatus(current.status) ? null : current;
    });
  }, []);

  const loadTapeFile = useCallback(() => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".tape";
    input.addEventListener("change", () => {
      const file = input.files?.[0];
      if (!file) return;
      void (async () => {
        try {
          const buf = await file.arrayBuffer();
          const bytes = new Uint8Array(buf);
          const tape = deserializeTape(bytes);
          setLatestRun({
            tape: bytes,
            score: tape.footer.finalScore,
            frameCount: tape.header.frameCount,
            seed: tape.header.seed,
            finalRngState: tape.footer.finalRngState,
            endedAtMs: Date.now(),
          });
          setProofError(null);
          setProofJob((current) =>
            current && isTerminalProofStatus(current.status) ? null : current,
          );
        } catch (error) {
          const detail = error instanceof Error ? error.message : String(error);
          setProofError(`failed to load tape file: ${detail}`);
        }
      })();
    });
    input.click();
  }, []);

  const submitLatestRun = useCallback(async () => {
    if (!latestRun) {
      return;
    }
    if (latestRun.score <= 0) {
      setProofError("zero-score runs are not accepted for proving or token minting");
      return;
    }

    setIsSubmitting(true);
    setProofError(null);

    try {
      const response = await submitProofJob(latestRun.tape);
      setProofJob(response.job);
    } catch (error) {
      if (error instanceof ProofApiError) {
        if (error.activeJob) {
          setProofJob(error.activeJob);
        }
        setProofError(error.message);
      } else {
        setProofError("failed to submit proof job");
      }
    } finally {
      setIsSubmitting(false);
    }
  }, [latestRun]);

  const connectWallet = useCallback(async () => {
    setWalletAction("connecting");
    setWalletError(null);

    try {
      const walletModule = await loadSmartWalletModule();
      const nextConfig = walletModule.getSmartAccountConfig();
      setWalletConfig({ networkPassphrase: nextConfig.networkPassphrase });
      setWalletRelayerMode(walletModule.getSmartAccountRelayerMode());
      const session = await walletModule.connectSmartWallet();
      setWalletSession(session);
      setProofError(null);
    } catch (error) {
      const detail = error instanceof Error ? error.message : "failed to connect wallet";
      setWalletError(detail);
    } finally {
      setWalletAction("idle");
    }
  }, []);

  const createWallet = useCallback(async () => {
    setWalletAction("creating");
    setWalletError(null);

    try {
      const walletModule = await loadSmartWalletModule();
      const nextConfig = walletModule.getSmartAccountConfig();
      setWalletConfig({ networkPassphrase: nextConfig.networkPassphrase });
      setWalletRelayerMode(walletModule.getSmartAccountRelayerMode());
      const session = await walletModule.createSmartWallet(walletUserName);
      setWalletSession(session);
      setProofError(null);
    } catch (error) {
      const detail = error instanceof Error ? error.message : "failed to create wallet";
      setWalletError(detail);
    } finally {
      setWalletAction("idle");
    }
  }, [walletUserName]);

  const disconnectWallet = useCallback(async () => {
    setWalletAction("disconnecting");
    setWalletError(null);

    try {
      const walletModule = await loadSmartWalletModule();
      const nextConfig = walletModule.getSmartAccountConfig();
      setWalletConfig({ networkPassphrase: nextConfig.networkPassphrase });
      setWalletRelayerMode(walletModule.getSmartAccountRelayerMode());
      await walletModule.disconnectSmartWallet();
      setWalletSession(null);
    } catch (error) {
      const detail = error instanceof Error ? error.message : "failed to disconnect wallet";
      setWalletError(detail);
    } finally {
      setWalletAction("idle");
    }
  }, []);

  const cancelActiveJob = useCallback(async () => {
    if (!proofJob || isTerminalProofStatus(proofJob.status)) {
      return;
    }

    try {
      const response = await cancelProofJob(proofJob.jobId);
      setProofJob(response.job);
      setProofError(null);
    } catch (error) {
      const message = error instanceof Error ? error.message : "failed to cancel job";
      setProofError(message);
    }
  }, [proofJob]);

  useEffect(() => {
    if (!activeProofJobId || !activeProofJobStatus || isTerminalProofStatus(activeProofJobStatus)) {
      return;
    }

    let cancelled = false;
    let timeoutId: number | null = null;

    const poll = async () => {
      try {
        const response = await getProofJob(activeProofJobId);
        if (cancelled) {
          return;
        }

        setProofJob(response.job);
        if (!isTerminalProofStatus(response.job.status)) {
          timeoutId = window.setTimeout(poll, 3000);
          return;
        }
      } catch (error) {
        if (cancelled) {
          return;
        }

        const message = error instanceof Error ? error.message : "failed to refresh proof status";
        setProofError(message);
        timeoutId = window.setTimeout(poll, 5000);
      }
    };

    timeoutId = window.setTimeout(poll, 1200);

    return () => {
      cancelled = true;
      if (timeoutId !== null) {
        window.clearTimeout(timeoutId);
      }
    };
  }, [activeProofJobId, activeProofJobStatus]);

  useEffect(() => {
    let cancelled = false;
    let timeoutId: number | null = null;

    const pollHealth = async () => {
      try {
        const response = await getGatewayHealth();
        if (cancelled) {
          return;
        }
        setGatewayHealth(response);
        setGatewayHealthError(null);
        if (response.active_job) {
          setProofJob((current) => current ?? response.active_job);
        }
      } catch (error) {
        if (cancelled) {
          return;
        }
        const message = error instanceof Error ? error.message : "failed to refresh gateway health";
        setGatewayHealthError(message);
      } finally {
        if (!cancelled) {
          timeoutId = window.setTimeout(pollHealth, 15_000);
        }
      }
    };

    timeoutId = window.setTimeout(pollHealth, 300);

    return () => {
      cancelled = true;
      if (timeoutId !== null) {
        window.clearTimeout(timeoutId);
      }
    };
  }, []);

  useEffect(() => {
    let cancelled = false;

    const restore = async () => {
      setWalletAction("restoring");
      setWalletError(null);

      try {
        const walletModule = await loadSmartWalletModule();
        const nextConfig = walletModule.getSmartAccountConfig();
        const nextRelayerMode = walletModule.getSmartAccountRelayerMode();
        const session = await walletModule.restoreSmartWalletSession();
        if (cancelled) {
          return;
        }
        setWalletConfig({ networkPassphrase: nextConfig.networkPassphrase });
        setWalletRelayerMode(nextRelayerMode);
        setWalletSession(session);
      } catch (error) {
        if (cancelled) {
          return;
        }
        const detail = error instanceof Error ? error.message : "failed to restore wallet session";
        setWalletError(detail);
      } finally {
        if (!cancelled) {
          setWalletAction("idle");
        }
      }
    };

    void restore();

    return () => {
      cancelled = true;
    };
  }, []);

  const proofBusy = proofJob ? !isTerminalProofStatus(proofJob.status) : false;
  const hasPositiveScore = (latestRun?.score ?? 0) > 0;
  const walletConnected = claimantAddress.trim().length > 0;
  const walletBusy = walletAction !== "idle";
  const canSubmit =
    Boolean(latestRun) &&
    hasPositiveScore &&
    !isSubmitting &&
    !proofBusy &&
    walletConnected &&
    !walletBusy;
  const currentStatus: ProofJobStatus | "idle" = proofJob ? proofJob.status : "idle";
  const currentStatusLabel = proofJob ? statusLabel(proofJob.status) : "Not Submitted";
  const proverHealthStatus = gatewayHealth?.prover.status ?? "degraded";
  const proverHealthClassName =
    proverHealthStatus === "compatible"
      ? "gateway-health gateway-health--ok"
      : "gateway-health gateway-health--warn";
  const walletStatusText = walletActionLabel(walletAction, walletConnected);
  const walletStatusClassName = walletChipClassName(walletAction, walletConnected);

  return (
    <main className="app-shell">
      <section className="headline">
        <h1>Asteroids Clone</h1>
        <p>
          Deterministic tape capture wired to a Cloudflare proof gateway. Game-over runs can be
          submitted and processed through a single-flight queue into the VAST prover API.
        </p>
      </section>

      <section className="game-panel" aria-label="Asteroids game panel">
        <AsteroidsCanvas onGameOver={handleGameOver} claimantAddress={claimantAddress} />
      </section>

      <section className="proof-panel" aria-live="polite">
        <div className="proof-panel__header">
          <h2>Proof Queue</h2>
          <span className={statusClassName(currentStatus)}>{currentStatusLabel}</span>
        </div>

        <p className="proof-panel__intro">
          The queue is intentionally single-active-job to match prover single-flight behavior.
        </p>
        <div className={proverHealthClassName}>
          <p>
            <strong>Gateway Health:</strong>{" "}
            {gatewayHealth ? (
              gatewayHealth.prover.status === "compatible" ? (
                <>compatible</>
              ) : (
                <>degraded</>
              )
            ) : (
              "loading"
            )}
          </p>
          {gatewayHealth?.prover.status === "compatible" ? (
            <>
              <p>
                <strong>Rules:</strong> {gatewayHealth.prover.ruleset} /{" "}
                {gatewayHealth.prover.rules_digest_hex.toUpperCase()}
              </p>
              <p>
                <strong>Prover Image:</strong>{" "}
                <code>{abbreviateHex(gatewayHealth.prover.image_id)}</code>
                {gatewayHealth.expected.image_id ? " (pinned)" : ""}
              </p>
            </>
          ) : null}
          {gatewayHealth?.prover.status === "degraded" ? (
            <p className="proof-warning">
              <strong>Health Error:</strong> {gatewayHealth.prover.error}
            </p>
          ) : null}
          {gatewayHealthError ? (
            <p className="proof-warning">
              <strong>Health Polling:</strong> {gatewayHealthError}
            </p>
          ) : null}
        </div>

        {latestRun ? (
          <dl className="proof-meta-grid">
            <div>
              <dt>Score</dt>
              <dd>{latestRun.score.toLocaleString()}</dd>
            </div>
            <div>
              <dt>Frames</dt>
              <dd>{latestRun.frameCount.toLocaleString()}</dd>
            </div>
            <div>
              <dt>Seed</dt>
              <dd>{formatHex32(latestRun.seed)}</dd>
            </div>
            <div>
              <dt>Final RNG</dt>
              <dd>{formatHex32(latestRun.finalRngState)}</dd>
            </div>
            <div>
              <dt>Tape Bytes</dt>
              <dd>{latestRun.tape.byteLength.toLocaleString()}</dd>
            </div>
            <div>
              <dt>Captured</dt>
              <dd>{new Date(latestRun.endedAtMs).toLocaleTimeString()}</dd>
            </div>
          </dl>
        ) : (
          <p className="proof-placeholder">Finish a run to capture a replay tape for proving.</p>
        )}
        {latestRun && latestRun.score <= 0 ? (
          <p className="proof-warning">
            Zero-score runs are not accepted for proving or token minting.
          </p>
        ) : null}
        {latestRun && !walletConnected ? (
          <p className="proof-warning">Connect a smart wallet before submitting a proof.</p>
        ) : null}

        <div className="wallet-panel">
          <div className="wallet-panel__header">
            <div className="wallet-panel__copy">
              <h3>Smart Wallet</h3>
              <p>Proof claims are locked to the connected smart-account contract address.</p>
            </div>
            <span className={walletStatusClassName}>{walletStatusText}</span>
          </div>

          {!walletConnected ? (
            <div className="wallet-panel__actions">
              <input
                type="text"
                placeholder="Username for passkey (optional)"
                value={walletUserName}
                onChange={(event) => setWalletUserName(event.target.value)}
                disabled={walletBusy}
              />
              <button type="button" onClick={createWallet} disabled={walletBusy}>
                {walletAction === "creating" ? "Creating Wallet..." : "Create Wallet"}
              </button>
              <button type="button" onClick={connectWallet} disabled={walletBusy}>
                {walletAction === "connecting" || walletAction === "restoring"
                  ? "Connecting..."
                  : "Connect Wallet"}
              </button>
            </div>
          ) : (
            <div className="wallet-panel__actions">
              <button type="button" onClick={disconnectWallet} disabled={walletBusy}>
                {walletAction === "disconnecting" ? "Disconnecting..." : "Disconnect Wallet"}
              </button>
            </div>
          )}

          <div className="claimant-field">
            <label htmlFor="claimant-address">Claimant Address</label>
            <input
              id="claimant-address"
              type="text"
              placeholder="Connect wallet to set claimant address"
              readOnly
              spellCheck={false}
              value={claimantAddress}
            />
          </div>

          <div className="wallet-panel__meta">
            <span>
              <strong>Network:</strong> {walletConfig.networkPassphrase}
            </span>
            <span>
              <strong>Relayer:</strong> {relayerModeLabel(walletRelayerMode)}
            </span>
          </div>

          {walletSession ? (
            <p className="wallet-panel__credential">
              <strong>Credential:</strong>{" "}
              <code>{abbreviateHex(walletSession.credentialId, 10)}</code>
            </p>
          ) : null}
          {walletError ? (
            <p className="proof-warning">
              <strong>Wallet:</strong> {walletError}
            </p>
          ) : null}
        </div>

        <div className="proof-actions">
          <button type="button" onClick={loadTapeFile} disabled={proofBusy}>
            Load Tape
          </button>
          <button type="button" onClick={submitLatestRun} disabled={!canSubmit}>
            {isSubmitting ? "Submitting Tape..." : "Submit For Proof"}
          </button>
          {proofJob?.result ? (
            <button
              type="button"
              onClick={async () => {
                const res = await fetch(`/api/proofs/jobs/${proofJob.jobId}/result`);
                const blob = new Blob([await res.text()], { type: "application/json" });
                const url = URL.createObjectURL(blob);
                window.open(url, "_blank");
                URL.revokeObjectURL(url);
              }}
            >
              Open Raw Proof JSON
            </button>
          ) : null}
        </div>

        {proofJob ? (
          <div className="proof-job-box">
            <p>
              <strong>Job ID:</strong> <code>{proofJob.jobId}</code>
            </p>
            <p>
              <strong>Queue Attempts:</strong> {proofJob.queue.attempts}
              {proofBusy ? (
                <button type="button" className="cancel-job-btn" onClick={cancelActiveJob}>
                  Cancel
                </button>
              ) : null}
            </p>
            {proofJob.queue.lastError ? (
              <p className="proof-warning">
                <strong>Last Retry Reason:</strong> {proofJob.queue.lastError}
              </p>
            ) : null}
            {proofJob.result?.summary ? (
              <div className="proof-result-box">
                <p>
                  <strong>Proof Time:</strong> {formatDuration(proofJob.result.summary.elapsedMs)}
                </p>
                <p>
                  <strong>Receipt:</strong>{" "}
                  {proofJob.result.summary.producedReceiptKind ??
                    proofJob.result.summary.requestedReceiptKind}
                </p>
                <p>
                  <strong>Verified Score:</strong>{" "}
                  {proofJob.result.summary.journal.final_score.toLocaleString()}
                </p>
                <p>
                  <strong>Verified Frames:</strong>{" "}
                  {proofJob.result.summary.journal.frame_count.toLocaleString()}
                </p>
                <p>
                  <strong>Segments:</strong>{" "}
                  {proofJob.result.summary.stats.segments.toLocaleString()}
                </p>
              </div>
            ) : null}
            {proofJob.error ? (
              <p className="proof-error-inline">
                <strong>Failure:</strong> {proofJob.error}
              </p>
            ) : null}
          </div>
        ) : null}

        {proofError ? <p className="proof-error">{proofError}</p> : null}
      </section>

      <section className="footnote">
        <p>
          Controls: <strong>Arrow Keys</strong> move and turn, <strong>Space</strong> fires,
          <strong> P</strong> pauses, <strong>R</strong> restarts, <strong>D</strong> saves a tape,
          <strong> Esc</strong> returns to menu.
        </p>
      </section>
    </main>
  );
}

export default App;
