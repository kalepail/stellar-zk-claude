import { lazy, Suspense, useCallback, useEffect, useState } from "react";
import { AsteroidsCanvas, type CompletedGameRun } from "./components/AsteroidsCanvas";
import {
  cancelProofJob,
  getGatewayHealth,
  getProofArtifact,
  ProofApiError,
  type ClaimStatus,
  type GatewayHealthResponse,
  getProofJob,
  isTerminalProofStatus,
  submitProofJob,
  type ProofJobPublic,
  type ProofJobStatus,
} from "./proof/api";
import { deserializeTape, serializeTape, TAPE_FOOTER_SIZE, TAPE_HEADER_SIZE } from "./game/tape";
import type {
  SmartAccountConfig,
  SmartAccountRelayerMode,
  SmartWalletSession,
} from "./wallet/smartAccount";
import {
  explainScoreSubmissionError,
  getScoreContractIdFromEnv,
  getTokenContractIdFromEnv,
  readTokenBalance,
  submitScoreTransaction,
} from "./chain/score";
import { extractGroth16SealFromArtifact, packJournalRaw } from "./proof/artifact";
import {
  GATEWAY_HEALTH_INITIAL_POLL_DELAY_MS,
  GATEWAY_HEALTH_POLL_INTERVAL_MS,
  PROOF_STATUS_ERROR_POLL_INTERVAL_MS,
  PROOF_STATUS_INITIAL_POLL_DELAY_MS,
  PROOF_STATUS_POLL_INTERVAL_MS,
  TESTNET_NETWORK_PASSPHRASE,
} from "./consts";
import "./App.css";
import { formatUtcDateTime } from "./time";

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

function formatWholeNumber(value: bigint): string {
  const sign = value < 0n ? "-" : "";
  const digits = (value < 0n ? -value : value).toString();
  return `${sign}${digits.replace(/\B(?=(\d{3})+(?!\d))/g, ",")}`;
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

function claimStatusLabel(
  status: "queued" | "submitting" | "retrying" | "succeeded" | "failed",
): string {
  switch (status) {
    case "queued":
      return "Queued";
    case "submitting":
      return "Submitting";
    case "retrying":
      return "Retrying";
    case "succeeded":
      return "Submitted";
    case "failed":
      return "Failed";
    default:
      return status;
  }
}

function isTerminalClaimStatus(status: ClaimStatus): boolean {
  return status === "succeeded" || status === "failed";
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
    case "configured":
      return "Relayer Configured";
    default:
      return "Not Configured (relayer required)";
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

const LazyLeaderboardPage = lazy(() =>
  import("./leaderboard/LeaderboardPage").then((m) => ({ default: m.LeaderboardPage })),
);

function GameApp() {
  const [latestRun, setLatestRun] = useState<CompletedGameRun | null>(null);
  const [proofJob, setProofJob] = useState<ProofJobPublic | null>(null);
  const [proofError, setProofError] = useState<string | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [walletSession, setWalletSession] = useState<SmartWalletSession | null>(null);
  const [walletAction, setWalletAction] = useState<WalletAction>("idle");
  const [walletUserName, setWalletUserName] = useState("");
  const [walletError, setWalletError] = useState<string | null>(null);
  const [walletConfig, setWalletConfig] = useState<Pick<SmartAccountConfig, "networkPassphrase">>({
    networkPassphrase: TESTNET_NETWORK_PASSPHRASE,
  });
  const [walletRelayerMode, setWalletRelayerMode] = useState<SmartAccountRelayerMode>("disabled");
  const [gatewayHealth, setGatewayHealth] = useState<GatewayHealthResponse | null>(null);
  const [gatewayHealthError, setGatewayHealthError] = useState<string | null>(null);
  const [manualClaimStatus, setManualClaimStatus] = useState<
    "idle" | "submitting" | "succeeded" | "failed"
  >("idle");
  const [manualClaimTxHash, setManualClaimTxHash] = useState<string | null>(null);
  const [manualClaimError, setManualClaimError] = useState<string | null>(null);
  const [tokenBalance, setTokenBalance] = useState<bigint | null>(null);
  const [tokenContractId, setTokenContractId] = useState<string | null>(null);
  const [tokenBalanceError, setTokenBalanceError] = useState<string | null>(null);
  const [isRefreshingBalance, setIsRefreshingBalance] = useState(false);
  const activeProofJobId = proofJob?.jobId ?? null;
  const activeProofJobStatus = proofJob?.status ?? null;
  const activeClaimStatus = proofJob?.claim.status ?? null;
  const claimantAddress = walletSession?.contractId ?? "";
  const scoreContractId = getScoreContractIdFromEnv();
  const tokenContractOverride = getTokenContractIdFromEnv();

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
            record: {
              seed: tape.header.seed,
              inputs: tape.inputs,
              finalScore: tape.footer.finalScore,
              finalRngState: tape.footer.finalRngState,
            },
            frameCount: tape.header.frameCount,
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
    if (latestRun.record.finalScore <= 0) {
      setProofError("zero-score runs are not accepted for proving or token minting");
      return;
    }
    if (claimantAddress.trim().length === 0) {
      setProofError("connect a smart wallet before submitting a proof");
      return;
    }

    let tapeBytes: Uint8Array;
    try {
      tapeBytes = serializeTape(
        latestRun.record.seed,
        latestRun.record.inputs,
        latestRun.record.finalScore,
        latestRun.record.finalRngState,
      );
    } catch (error) {
      const message = error instanceof Error ? error.message : "failed to serialize tape";
      setProofError(message);
      return;
    }

    setIsSubmitting(true);
    setProofError(null);

    try {
      const response = await submitProofJob(tapeBytes, claimantAddress);
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
  }, [claimantAddress, latestRun]);

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

  const refreshBalance = useCallback(async () => {
    if (claimantAddress.trim().length === 0) {
      setTokenBalance(null);
      setTokenContractId(null);
      setTokenBalanceError(null);
      return;
    }

    if (!scoreContractId && !tokenContractOverride) {
      setTokenBalance(null);
      setTokenContractId(null);
      setTokenBalanceError(
        "set VITE_SCORE_CONTRACT_ID (or VITE_TOKEN_CONTRACT_ID) to show on-chain token balance",
      );
      return;
    }

    setIsRefreshingBalance(true);
    try {
      const next = await readTokenBalance({
        walletAddress: claimantAddress,
        scoreContractId,
        tokenContractId: tokenContractOverride,
      });
      setTokenBalance(next.balance);
      setTokenContractId(next.tokenContractId);
      setTokenBalanceError(null);
    } catch (error) {
      const detail = error instanceof Error ? error.message : "failed to load token balance";
      setTokenBalanceError(detail);
    } finally {
      setIsRefreshingBalance(false);
    }
  }, [claimantAddress, scoreContractId, tokenContractOverride]);

  const submitProvenScoreOnChain = useCallback(async () => {
    if (!proofJob?.result?.summary) {
      setManualClaimStatus("failed");
      setManualClaimError("proof result is not available yet");
      return;
    }

    if (claimantAddress.trim().length === 0) {
      setManualClaimStatus("failed");
      setManualClaimError("connect a smart wallet before submitting on-chain");
      return;
    }

    if (!scoreContractId) {
      setManualClaimStatus("failed");
      setManualClaimError("missing VITE_SCORE_CONTRACT_ID in frontend env");
      return;
    }

    setManualClaimStatus("submitting");
    setManualClaimError(null);
    setManualClaimTxHash(null);

    try {
      const artifact = await getProofArtifact(proofJob.jobId);
      const seal = extractGroth16SealFromArtifact(artifact);
      const journalRaw = packJournalRaw(proofJob.result.summary.journal);

      if (walletRelayerMode === "disabled") {
        throw new Error("relayer is not configured for this wallet session");
      }

      const tx = await submitScoreTransaction({
        scoreContractId,
        claimantAddress,
        seal,
        journalRaw,
      });

      if (!tx.success) {
        throw new Error(tx.error ?? "on-chain submission failed");
      }

      setManualClaimStatus("succeeded");
      setManualClaimTxHash(tx.hash || null);
      setManualClaimError(null);
      void refreshBalance();
    } catch (error) {
      const detail = error instanceof Error ? error.message : "on-chain submission failed";
      setManualClaimStatus("failed");
      setManualClaimError(explainScoreSubmissionError(detail));
    }
  }, [
    claimantAddress,
    proofJob,
    refreshBalance,
    scoreContractId,
    walletRelayerMode,
  ]);

  useEffect(() => {
    if (!activeProofJobId || !activeProofJobStatus) {
      return;
    }
    const keepPolling =
      !isTerminalProofStatus(activeProofJobStatus) ||
      (activeProofJobStatus === "succeeded" &&
        activeClaimStatus !== null &&
        !isTerminalClaimStatus(activeClaimStatus));
    if (!keepPolling) {
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
        const shouldContinuePolling =
          !isTerminalProofStatus(response.job.status) ||
          (response.job.status === "succeeded" &&
            !isTerminalClaimStatus(response.job.claim.status));
        if (shouldContinuePolling) {
          timeoutId = window.setTimeout(poll, PROOF_STATUS_POLL_INTERVAL_MS);
          return;
        }
      } catch (error) {
        if (cancelled) {
          return;
        }

        const message = error instanceof Error ? error.message : "failed to refresh proof status";
        setProofError(message);
        timeoutId = window.setTimeout(poll, PROOF_STATUS_ERROR_POLL_INTERVAL_MS);
      }
    };

    timeoutId = window.setTimeout(poll, PROOF_STATUS_INITIAL_POLL_DELAY_MS);

    return () => {
      cancelled = true;
      if (timeoutId !== null) {
        window.clearTimeout(timeoutId);
      }
    };
  }, [activeClaimStatus, activeProofJobId, activeProofJobStatus]);

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
          timeoutId = window.setTimeout(pollHealth, GATEWAY_HEALTH_POLL_INTERVAL_MS);
        }
      }
    };

    timeoutId = window.setTimeout(pollHealth, GATEWAY_HEALTH_INITIAL_POLL_DELAY_MS);

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

  useEffect(() => {
    setManualClaimStatus("idle");
    setManualClaimError(null);
    setManualClaimTxHash(null);
  }, [proofJob?.jobId]);

  useEffect(() => {
    if (proofJob?.claim.status === "succeeded") {
      setManualClaimStatus("succeeded");
      setManualClaimError(null);
      if (proofJob.claim.txHash) {
        setManualClaimTxHash(proofJob.claim.txHash);
      }
      void refreshBalance();
    }
  }, [proofJob?.claim.status, proofJob?.claim.txHash, refreshBalance]);

  useEffect(() => {
    if (claimantAddress.trim().length === 0) {
      setTokenBalance(null);
      setTokenContractId(null);
      setTokenBalanceError(null);
      return;
    }

    void refreshBalance();
  }, [claimantAddress, refreshBalance]);

  const proofBusy = proofJob ? !isTerminalProofStatus(proofJob.status) : false;
  const claimBusy = manualClaimStatus === "submitting";
  const hasProofResult = Boolean(proofJob?.result?.summary);
  const hasPositiveScore = (latestRun?.record.finalScore ?? 0) > 0;
  const walletConnected = claimantAddress.trim().length > 0;
  const walletBusy = walletAction !== "idle";
  const canSubmit =
    Boolean(latestRun) &&
    hasPositiveScore &&
    !isSubmitting &&
    !proofBusy &&
    walletConnected &&
    !walletBusy;
  const canSubmitOnChain =
    hasProofResult && walletConnected && !walletBusy && !claimBusy && Boolean(scoreContractId);
  const currentStatus: ProofJobStatus | "idle" = proofJob ? proofJob.status : "idle";
  const currentStatusLabel = proofJob ? statusLabel(proofJob.status) : "Not Submitted";
  const proverHealthStatus = gatewayHealth?.prover.status ?? "degraded";
  const proverHealthClassName =
    proverHealthStatus === "compatible"
      ? "gateway-health gateway-health--ok"
      : "gateway-health gateway-health--warn";
  const walletStatusText = walletActionLabel(walletAction, walletConnected);
  const walletStatusClassName = walletChipClassName(walletAction, walletConnected);
  const balanceLabel =
    tokenBalance === null
      ? "—"
      : `${formatWholeNumber(tokenBalance)} score token${tokenBalance === 1n ? "" : "s"}`;

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
        <AsteroidsCanvas onGameOver={handleGameOver} />
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
              <dd>{latestRun.record.finalScore.toLocaleString()}</dd>
            </div>
            <div>
              <dt>Frames</dt>
              <dd>{latestRun.frameCount.toLocaleString()}</dd>
            </div>
            <div>
              <dt>Seed</dt>
              <dd>{formatHex32(latestRun.record.seed)}</dd>
            </div>
            <div>
              <dt>Final RNG</dt>
              <dd>{formatHex32(latestRun.record.finalRngState)}</dd>
            </div>
            <div>
              <dt>Tape Bytes</dt>
              <dd>
                {(TAPE_HEADER_SIZE + latestRun.frameCount + TAPE_FOOTER_SIZE).toLocaleString()}
              </dd>
            </div>
            <div>
              <dt>Captured</dt>
              <dd>{formatUtcDateTime(latestRun.endedAtMs)}</dd>
            </div>
          </dl>
        ) : (
          <p className="proof-placeholder">Finish a run to capture a replay tape for proving.</p>
        )}
        {latestRun && latestRun.record.finalScore <= 0 ? (
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
              <p>Proof claims are relayed on-chain to the connected smart-account address.</p>
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

          <div className="wallet-balance">
            <div className="wallet-balance__header">
              <p>
                <strong>Won Balance:</strong> {balanceLabel}
              </p>
              <button
                type="button"
                onClick={() => void refreshBalance()}
                disabled={!walletConnected || isRefreshingBalance}
              >
                {isRefreshingBalance ? "Refreshing..." : "Refresh Balance"}
              </button>
            </div>
            {tokenContractId ? (
              <p className="wallet-balance__contract">
                <strong>Token Contract:</strong> <code>{abbreviateHex(tokenContractId, 10)}</code>
              </p>
            ) : null}
            {tokenBalanceError ? (
              <p className="proof-warning">
                <strong>Balance:</strong> {tokenBalanceError}
              </p>
            ) : null}
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
          {hasProofResult ? (
            <button type="button" onClick={submitProvenScoreOnChain} disabled={!canSubmitOnChain}>
              {manualClaimStatus === "submitting"
                ? "Submitting On-chain..."
                : "Submit Proven Score On-chain"}
            </button>
          ) : null}
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
              <strong>Created:</strong> {formatUtcDateTime(proofJob.createdAt)}
            </p>
            <p>
              <strong>Updated:</strong> {formatUtcDateTime(proofJob.updatedAt)}
            </p>
            {proofJob.completedAt ? (
              <p>
                <strong>Completed:</strong> {formatUtcDateTime(proofJob.completedAt)}
              </p>
            ) : null}
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
                <p>
                  <strong>Claim:</strong> {claimStatusLabel(proofJob.claim.status)}
                </p>
                {proofJob.claim.txHash ? (
                  <p>
                    <strong>Tx Hash:</strong> <code>{proofJob.claim.txHash}</code>
                  </p>
                ) : null}
                <p>
                  <strong>Manual Submit:</strong>{" "}
                  {manualClaimStatus === "idle"
                    ? "not submitted"
                    : manualClaimStatus === "submitting"
                      ? "submitting"
                      : manualClaimStatus}
                </p>
                <p>
                  <strong>Manual Path:</strong> Relayer (Fee Sponsored)
                </p>
                {manualClaimTxHash ? (
                  <p>
                    <strong>Manual Tx:</strong> <code>{manualClaimTxHash}</code>
                  </p>
                ) : null}
                {manualClaimError ? (
                  <p className="proof-warning">
                    <strong>Manual Claim:</strong> {manualClaimError}
                  </p>
                ) : null}
                {!scoreContractId ? (
                  <p className="proof-warning">
                    <strong>Manual Claim:</strong> set VITE_SCORE_CONTRACT_ID in frontend env
                  </p>
                ) : null}
              </div>
            ) : null}
            {proofJob.claim.lastError ? (
              <p className="proof-warning">
                <strong>Auto Claim:</strong> {proofJob.claim.lastError}
              </p>
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

function App() {
  if (window.location.pathname.startsWith("/leaderboard")) {
    return (
      <Suspense>
        <LazyLeaderboardPage />
      </Suspense>
    );
  }

  return <GameApp />;
}

export default App;
