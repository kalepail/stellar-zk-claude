import { useCallback, useEffect, useState } from "react";
import { AsteroidsCanvas, type CompletedGameRun } from "./components/AsteroidsCanvas";
import {
  ProofApiError,
  getProofJob,
  isTerminalProofStatus,
  submitProofJob,
  type ProofJobPublic,
  type ProofJobStatus,
} from "./proof/api";
import "./App.css";

function formatHex32(value: number): string {
  return `0x${(value >>> 0).toString(16).toUpperCase().padStart(8, "0")}`;
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

function App() {
  const [latestRun, setLatestRun] = useState<CompletedGameRun | null>(null);
  const [proofJob, setProofJob] = useState<ProofJobPublic | null>(null);
  const [proofError, setProofError] = useState<string | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const activeProofJobId = proofJob?.jobId ?? null;
  const activeProofJobStatus = proofJob?.status ?? null;

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

  const submitLatestRun = useCallback(async () => {
    if (!latestRun) {
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

  const proofBusy = proofJob ? !isTerminalProofStatus(proofJob.status) : false;
  const canSubmit = Boolean(latestRun) && !isSubmitting && !proofBusy;
  const currentStatus: ProofJobStatus | "idle" = proofJob ? proofJob.status : "idle";
  const currentStatusLabel = proofJob ? statusLabel(proofJob.status) : "Not Submitted";

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

        <div className="proof-actions">
          <button type="button" onClick={submitLatestRun} disabled={!canSubmit}>
            {isSubmitting ? "Submitting Tape..." : "Submit Latest Run For Proof"}
          </button>
          {proofJob?.result ? (
            <a href={`/api/proofs/jobs/${proofJob.jobId}/result`} target="_blank" rel="noreferrer">
              Open Raw Proof JSON
            </a>
          ) : null}
        </div>

        {proofJob ? (
          <div className="proof-job-box">
            <p>
              <strong>Job ID:</strong> <code>{proofJob.jobId}</code>
            </p>
            <p>
              <strong>Queue Attempts:</strong> {proofJob.queue.attempts}
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
