import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  type ClaimStatus,
  getLeaderboard,
  getLeaderboardPlayer,
  LeaderboardApiError,
  type LeaderboardEntry,
  type LeaderboardPageResponse,
  type LeaderboardPlayerResponse,
  type LeaderboardWindow,
  updateLeaderboardProfile,
} from "./api";
import { formatUtcDateTime, timeAgo } from "../time";
import "./LeaderboardPage.css";

const AUTO_REFRESH_MS = 60_000;

function abbreviateAddress(value: string): string {
  if (value.length <= 16) {
    return value;
  }
  return `${value.slice(0, 8)}...${value.slice(-8)}`;
}

function formatHex32(value: number): string {
  return `0x${(value >>> 0).toString(16).toUpperCase().padStart(8, "0")}`;
}

function displayName(entry: LeaderboardEntry): string {
  return entry.profile?.username?.trim() || abbreviateAddress(entry.claimantAddress);
}

function getPlayerAddressFromPath(pathname: string): string | null {
  const segments = pathname.split("/").filter(Boolean);
  if (segments[0] !== "leaderboard") {
    return null;
  }

  return segments[1] ?? null;
}

function toNullableTrimmed(value: string): string | null {
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

function windowLabel(window: LeaderboardWindow): string {
  if (window === "10m") {
    return "10m";
  }
  if (window === "day") {
    return "24h";
  }
  return "All";
}

function windowSubtitle(window: LeaderboardWindow): string {
  if (window === "10m") {
    return "Last 10 minutes";
  }
  if (window === "day") {
    return "Last 24 hours";
  }
  return "All-time history";
}

function claimStatusClass(status: ClaimStatus): string {
  return `leaderboard-status leaderboard-status--${status}`;
}

function rankClass(rank: number): string {
  if (rank === 1) {
    return "leaderboard-rank leaderboard-rank--top1";
  }
  if (rank === 2) {
    return "leaderboard-rank leaderboard-rank--top2";
  }
  if (rank === 3) {
    return "leaderboard-rank leaderboard-rank--top3";
  }
  return "leaderboard-rank";
}

function formatMetric(value: number | null | undefined): string {
  return typeof value === "number" && Number.isFinite(value) ? value.toLocaleString() : "n/a";
}

function isSmartAccountContractAddress(address: string): boolean {
  return address.trim().startsWith("C");
}

function isSafeUrl(url: string | null | undefined): boolean {
  if (!url) {
    return false;
  }
  const trimmed = url.trim().toLowerCase();
  return trimmed.startsWith("http://") || trimmed.startsWith("https://");
}

function RelativeTime({ value }: { value: string | null | undefined }) {
  const [, forceUpdate] = useState(0);

  useEffect(() => {
    if (!value) {
      return;
    }
    const interval = setInterval(() => forceUpdate((n) => n + 1), 15_000);
    return () => clearInterval(interval);
  }, [value]);

  if (!value) {
    return <span>n/a</span>;
  }

  return <span title={formatUtcDateTime(value)}>{timeAgo(value)}</span>;
}

function SkeletonRows({ count }: { count: number }) {
  return (
    <>
      {Array.from({ length: count }, (_, i) => (
        <tr key={i} className="leaderboard-skeleton-row">
          <td>
            <span className="leaderboard-skeleton-cell" />
          </td>
          <td>
            <span className="leaderboard-skeleton-cell leaderboard-skeleton-cell--wide" />
          </td>
          <td>
            <span className="leaderboard-skeleton-cell" />
          </td>
          <td>
            <span className="leaderboard-skeleton-cell" />
          </td>
          <td>
            <span className="leaderboard-skeleton-cell" />
          </td>
          <td>
            <span className="leaderboard-skeleton-cell" />
          </td>
          <td>
            <span className="leaderboard-skeleton-cell leaderboard-skeleton-cell--wide" />
          </td>
          <td>
            <span className="leaderboard-skeleton-cell" />
          </td>
        </tr>
      ))}
    </>
  );
}

function PlayerSkeletonRows({ count }: { count: number }) {
  return (
    <>
      {Array.from({ length: count }, (_, i) => (
        <tr key={i} className="leaderboard-skeleton-row">
          <td>
            <span className="leaderboard-skeleton-cell" />
          </td>
          <td>
            <span className="leaderboard-skeleton-cell" />
          </td>
          <td>
            <span className="leaderboard-skeleton-cell" />
          </td>
          <td>
            <span className="leaderboard-skeleton-cell" />
          </td>
          <td>
            <span className="leaderboard-skeleton-cell leaderboard-skeleton-cell--wide" />
          </td>
          <td>
            <span className="leaderboard-skeleton-cell" />
          </td>
        </tr>
      ))}
    </>
  );
}

export function LeaderboardPage() {
  const pathname = typeof window !== "undefined" ? window.location.pathname : "/leaderboard";
  const playerAddress = useMemo(() => getPlayerAddressFromPath(pathname), [pathname]);

  const [windowKey, setWindowKey] = useState<LeaderboardWindow>("all");
  const [offset, setOffset] = useState(0);
  const [limit] = useState(25);
  const [searchInput, setSearchInput] = useState("");
  const [findAddress, setFindAddress] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [leaderboard, setLeaderboard] = useState<LeaderboardPageResponse | null>(null);

  const [playerLoading, setPlayerLoading] = useState(false);
  const [playerError, setPlayerError] = useState<string | null>(null);
  const [playerData, setPlayerData] = useState<LeaderboardPlayerResponse | null>(null);
  const [profileUsername, setProfileUsername] = useState("");
  const [profileLinkUrl, setProfileLinkUrl] = useState("");
  const [savingProfile, setSavingProfile] = useState(false);
  const [profileSaveError, setProfileSaveError] = useState<string | null>(null);
  const [profileSavedAt, setProfileSavedAt] = useState<string | null>(null);

  // Track last refresh time for relative display
  const [lastRefreshAt, setLastRefreshAt] = useState<string | null>(null);

  const fetchLeaderboardRef = useRef<(() => void) | undefined>(undefined);

  const fetchLeaderboard = useCallback(
    (silent: boolean) => {
      if (playerAddress) {
        return;
      }

      if (!silent) {
        setLoading(true);
        setError(null);
      }

      void (async () => {
        try {
          const response = await getLeaderboard({
            window: windowKey,
            limit,
            offset,
            address: findAddress,
          });
          setLeaderboard(response);
          setLastRefreshAt(new Date().toISOString());
          if (!silent) {
            setError(null);
          }
        } catch (reason) {
          if (!silent) {
            const detail =
              reason instanceof LeaderboardApiError || reason instanceof Error
                ? reason.message
                : "failed to load leaderboard";
            setError(detail);
          }
        } finally {
          if (!silent) {
            setLoading(false);
          }
        }
      })();
    },
    [findAddress, limit, offset, playerAddress, windowKey],
  );

  fetchLeaderboardRef.current = () => fetchLeaderboard(true);

  // Initial + parameter-change fetch
  useEffect(() => {
    if (playerAddress) {
      return;
    }

    let cancelled = false;
    setLoading(true);
    setError(null);

    void (async () => {
      try {
        const response = await getLeaderboard({
          window: windowKey,
          limit,
          offset,
          address: findAddress,
        });
        if (!cancelled) {
          setLeaderboard(response);
          setLastRefreshAt(new Date().toISOString());
        }
      } catch (reason) {
        if (cancelled) {
          return;
        }
        const detail =
          reason instanceof LeaderboardApiError || reason instanceof Error
            ? reason.message
            : "failed to load leaderboard";
        setError(detail);
      } finally {
        if (!cancelled) {
          setLoading(false);
        }
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [findAddress, limit, offset, playerAddress, windowKey]);

  // Auto-refresh every 60s (silent background refresh)
  useEffect(() => {
    if (playerAddress) {
      return;
    }

    const interval = setInterval(() => {
      fetchLeaderboardRef.current?.();
    }, AUTO_REFRESH_MS);

    return () => clearInterval(interval);
  }, [playerAddress]);

  useEffect(() => {
    if (!playerAddress) {
      return;
    }

    let cancelled = false;
    setPlayerLoading(true);
    setPlayerError(null);
    setProfileSaveError(null);
    setProfileSavedAt(null);

    void (async () => {
      try {
        const response = await getLeaderboardPlayer(playerAddress);
        if (cancelled) {
          return;
        }
        setPlayerData(response);
        setProfileUsername(response.player.profile?.username ?? "");
        setProfileLinkUrl(response.player.profile?.linkUrl ?? "");
      } catch (reason) {
        if (cancelled) {
          return;
        }
        const detail =
          reason instanceof LeaderboardApiError || reason instanceof Error
            ? reason.message
            : "failed to load player";
        setPlayerError(detail);
      } finally {
        if (!cancelled) {
          setPlayerLoading(false);
        }
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [playerAddress]);

  const applyFindMe = useCallback(() => {
    const next = toNullableTrimmed(searchInput);
    setFindAddress(next);
    setOffset(0);
  }, [searchInput]);

  const clearFindMe = useCallback(() => {
    setSearchInput("");
    setFindAddress(null);
    setOffset(0);
  }, []);

  const saveProfile = useCallback(async () => {
    if (!playerData) {
      return;
    }

    const claimantAddress = playerData.player.claimant_address;
    if (!isSmartAccountContractAddress(claimantAddress)) {
      setProfileSavedAt(null);
      setProfileSaveError(
        "profile edits are only supported for smart-account claimant contract addresses",
      );
      return;
    }

    setSavingProfile(true);
    setProfileSaveError(null);
    setProfileSavedAt(null);

    try {
      const walletModule = await import("../wallet/smartAccount");
      const walletSession =
        await walletModule.resolveSmartWalletSessionForClaimant(claimantAddress);
      if (!walletSession.credentialPublicKey) {
        throw new Error("missing passkey public key in wallet session");
      }
      const updated = await updateLeaderboardProfile(
        claimantAddress,
        {
          username: toNullableTrimmed(profileUsername),
          linkUrl: toNullableTrimmed(profileLinkUrl),
        },
        {
          credentialId: walletSession.credentialId,
          credentialPublicKey: walletSession.credentialPublicKey,
          credentialTransports: walletSession.credentialTransports,
        },
      );

      setPlayerData((current) => {
        if (!current) {
          return current;
        }
        return {
          ...current,
          player: {
            ...current.player,
            profile: updated.profile,
          },
        };
      });
      setProfileSavedAt(updated.profile.updatedAt);
    } catch (reason) {
      const detail =
        reason instanceof LeaderboardApiError || reason instanceof Error
          ? reason.message
          : "failed to save profile";
      setProfileSaveError(detail);
    } finally {
      setSavingProfile(false);
    }
  }, [playerData, profileLinkUrl, profileUsername]);

  const supportsPlayerProfileAuth =
    playerData !== null && isSmartAccountContractAddress(playerData.player.claimant_address);

  if (playerAddress) {
    return (
      <main className="leaderboard-shell">
        <header className="leaderboard-header leaderboard-surface leaderboard-surface--hero">
          <div>
            <h1>Player</h1>
            <p>Profile, rankings, and recent proved runs.</p>
          </div>
          <a className="leaderboard-navlink" href="/leaderboard">
            Back To Leaderboard
          </a>
        </header>

        {playerLoading ? (
          <section className="leaderboard-card">
            <div className="leaderboard-table-wrap">
              <table className="leaderboard-table" aria-label="Loading player data">
                <tbody>
                  <PlayerSkeletonRows count={3} />
                </tbody>
              </table>
            </div>
          </section>
        ) : null}
        {playerError ? <p className="leaderboard-error">{playerError}</p> : null}

        {playerData ? (
          <>
            <section className="leaderboard-card">
              <h2>
                {playerData.player.profile?.username ??
                  abbreviateAddress(playerData.player.claimant_address)}
              </h2>
              <p>
                <strong>Address:</strong>{" "}
                <code className="leaderboard-address">{playerData.player.claimant_address}</code>
              </p>
              {playerData.player.profile?.linkUrl &&
              isSafeUrl(playerData.player.profile.linkUrl) ? (
                <p>
                  <strong>Link:</strong>{" "}
                  <a href={playerData.player.profile.linkUrl} target="_blank" rel="noreferrer">
                    {playerData.player.profile.linkUrl}
                  </a>
                </p>
              ) : null}
              <dl className="leaderboard-grid">
                <div>
                  <dt>Total Runs</dt>
                  <dd>{playerData.player.stats.total_runs.toLocaleString()}</dd>
                </div>
                <div>
                  <dt>Best Score</dt>
                  <dd>{playerData.player.stats.best_score.toLocaleString()}</dd>
                </div>
                <div>
                  <dt>Total Minted</dt>
                  <dd>{formatMetric(playerData.player.stats.total_minted)}</dd>
                </div>
                <div>
                  <dt>Last Played</dt>
                  <dd>
                    <RelativeTime value={playerData.player.stats.last_played_at} />
                  </dd>
                </div>
              </dl>
              <p className="leaderboard-note">
                Leaderboard rank uses each claimant's single best proved run in the selected window;
                this page also shows your full recent run history and total minted.
              </p>
              <dl className="leaderboard-grid">
                <div>
                  <dt>10m Rank</dt>
                  <dd>{playerData.player.ranks.ten_min ?? "n/a"}</dd>
                </div>
                <div>
                  <dt>24h Rank</dt>
                  <dd>{playerData.player.ranks.day ?? "n/a"}</dd>
                </div>
                <div>
                  <dt>All-Time Rank</dt>
                  <dd>{playerData.player.ranks.all ?? "n/a"}</dd>
                </div>
              </dl>
            </section>

            {supportsPlayerProfileAuth ? (
              <section className="leaderboard-card">
                <h3>Edit Profile</h3>
                <p className="leaderboard-note">
                  Saving requires a passkey prompt for the claimant wallet tied to this address.
                </p>
                <div className="leaderboard-form-grid">
                  <label>
                    Username
                    <input
                      type="text"
                      value={profileUsername}
                      onChange={(event) => setProfileUsername(event.target.value)}
                      placeholder="Your leaderboard name"
                      maxLength={32}
                    />
                  </label>
                  <label>
                    Link URL
                    <input
                      type="url"
                      value={profileLinkUrl}
                      onChange={(event) => setProfileLinkUrl(event.target.value)}
                      placeholder="https://"
                      maxLength={240}
                    />
                  </label>
                </div>
                <div className="leaderboard-actions">
                  <button type="button" onClick={saveProfile} disabled={savingProfile}>
                    {savingProfile ? "Saving..." : "Save Profile"}
                  </button>
                  {profileSavedAt ? (
                    <span className="leaderboard-note">
                      Saved <RelativeTime value={profileSavedAt} />
                    </span>
                  ) : null}
                </div>
                {profileSaveError ? <p className="leaderboard-error">{profileSaveError}</p> : null}
              </section>
            ) : (
              <section className="leaderboard-card">
                <h3>Edit Profile</h3>
                <p className="leaderboard-note">
                  Profile edits are available only for smart-account claimant contract addresses.
                </p>
              </section>
            )}

            <section className="leaderboard-card">
              <h3>Recent Runs</h3>
              <p className="leaderboard-note">
                Recent runs includes every proved submission for this claimant (not just the best
                run).
              </p>
              {playerData.player.recent_runs.length === 0 ? (
                <p className="leaderboard-note">No proved runs yet.</p>
              ) : (
                <div className="leaderboard-table-wrap">
                  <table className="leaderboard-table" aria-label="Recent proved runs">
                    <thead>
                      <tr>
                        <th scope="col">Score</th>
                        <th scope="col">Frames</th>
                        <th scope="col">Minted (this run)</th>
                        <th scope="col">Seed</th>
                        <th scope="col">Completed</th>
                        <th scope="col">Claim</th>
                      </tr>
                    </thead>
                    <tbody>
                      {playerData.player.recent_runs.map((run) => (
                        <tr key={run.jobId}>
                          <td className="leaderboard-cell--num">{run.score.toLocaleString()}</td>
                          <td className="leaderboard-cell--num">{formatMetric(run.frameCount)}</td>
                          <td className="leaderboard-cell--num">{formatMetric(run.mintedDelta)}</td>
                          <td className="leaderboard-cell--mono">{formatHex32(run.seed)}</td>
                          <td>
                            <RelativeTime value={run.completedAt} />
                          </td>
                          <td>
                            <span className={claimStatusClass(run.claimStatus)}>
                              {run.claimStatus}
                            </span>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </section>
          </>
        ) : null}
      </main>
    );
  }

  const showingStart = leaderboard
    ? Math.min(leaderboard.pagination.total, leaderboard.pagination.offset + 1)
    : 0;
  const showingEnd = leaderboard
    ? Math.min(
        leaderboard.pagination.total,
        leaderboard.pagination.offset + leaderboard.entries.length,
      )
    : 0;
  const topEntry = leaderboard?.entries[0] ?? null;
  const hasHistoricalData =
    (leaderboard?.window !== "all" &&
      leaderboard?.entries.length === 0 &&
      (leaderboard?.ingestion?.total_events ?? 0) > 0) ||
    false;
  const isEmptyAllTime =
    leaderboard?.window === "all" &&
    leaderboard.entries.length === 0 &&
    (leaderboard.ingestion?.total_events ?? 0) === 0;

  return (
    <main className="leaderboard-shell">
      <header className="leaderboard-header leaderboard-surface leaderboard-surface--hero">
        <div>
          <h1>Leaderboard</h1>
          <p>Rolling 10m, 24h, and all-time rankings from proved runs.</p>
          <p className="leaderboard-note">
            {leaderboard
              ? `${windowSubtitle(leaderboard.window)} window`
              : "Loading current ranking window"}
          </p>
        </div>
        <div className="leaderboard-header-actions">
          {leaderboard?.ingestion?.last_synced_at ? (
            <span
              className="leaderboard-sync-pill"
              title={formatUtcDateTime(leaderboard.ingestion.last_synced_at)}
            >
              Synced <RelativeTime value={leaderboard.ingestion.last_synced_at} />
            </span>
          ) : (
            <span className="leaderboard-sync-pill leaderboard-sync-pill--muted">
              Sync in progress
            </span>
          )}
          <div className="leaderboard-header-links">
            <button
              type="button"
              className="leaderboard-refresh-btn"
              onClick={() => fetchLeaderboard(false)}
              disabled={loading}
              title={lastRefreshAt ? `Last refreshed ${timeAgo(lastRefreshAt)}` : "Refresh"}
            >
              Refresh
            </button>
            <a className="leaderboard-navlink" href="/">
              Back To Game
            </a>
          </div>
        </div>
      </header>

      <section className="leaderboard-controls leaderboard-surface">
        <div className="leaderboard-controls-copy">
          <h2>Filters</h2>
          <p>Switch horizon or lookup a claimant contract address.</p>
        </div>
        <div className="leaderboard-window-buttons" role="group" aria-label="Time window selector">
          {(["10m", "day", "all"] as LeaderboardWindow[]).map((w) => (
            <button
              key={w}
              type="button"
              onClick={() => {
                setWindowKey(w);
                setOffset(0);
              }}
              className={w === windowKey ? "active" : ""}
              aria-pressed={w === windowKey}
            >
              {windowLabel(w)}
            </button>
          ))}
        </div>

        <form
          className="leaderboard-find-me"
          onSubmit={(event) => {
            event.preventDefault();
            applyFindMe();
          }}
        >
          <input
            type="text"
            value={searchInput}
            onChange={(event) => setSearchInput(event.target.value)}
            placeholder="Find address (G... or C...)"
            aria-label="Search for a player address"
          />
          <button type="submit">Find Me</button>
          <button type="button" onClick={clearFindMe} disabled={!findAddress}>
            Clear
          </button>
        </form>
      </section>

      {error ? <p className="leaderboard-error">{error}</p> : null}

      {loading && !leaderboard ? (
        <section className="leaderboard-card leaderboard-surface">
          <h2 className="leaderboard-section-title">Rankings</h2>
          <div className="leaderboard-table-wrap">
            <table className="leaderboard-table" aria-label="Loading leaderboard rankings">
              <thead>
                <tr>
                  <th scope="col">Rank</th>
                  <th scope="col">Player</th>
                  <th scope="col">Score</th>
                  <th scope="col">Frames</th>
                  <th scope="col">Minted (this run)</th>
                  <th scope="col">Seed</th>
                  <th scope="col">Completed</th>
                  <th scope="col">Claim</th>
                </tr>
              </thead>
              <tbody>
                <SkeletonRows count={5} />
              </tbody>
            </table>
          </div>
        </section>
      ) : null}

      {leaderboard ? (
        <>
          <section className="leaderboard-card leaderboard-surface">
            <div className="leaderboard-summary-line">
              <p>
                <strong>Window:</strong> {windowLabel(leaderboard.window)}
                {" · "}
                <strong>Updated:</strong> <RelativeTime value={leaderboard.generated_at} />
              </p>
              <p>
                <strong>Showing:</strong> {showingStart}-{showingEnd} of{" "}
                {leaderboard.pagination.total.toLocaleString()} players
              </p>
            </div>

            <dl className="leaderboard-kpi-grid">
              <div>
                <dt>Tracked Players</dt>
                <dd>{leaderboard.pagination.total.toLocaleString()}</dd>
              </div>
              <div>
                <dt>Top Score</dt>
                <dd>{topEntry ? topEntry.score.toLocaleString() : "n/a"}</dd>
              </div>
              <div>
                <dt>Event Rows</dt>
                <dd>{leaderboard.ingestion?.total_events?.toLocaleString() ?? "n/a"}</dd>
              </div>
              <div>
                <dt>Highest Ledger</dt>
                <dd>{leaderboard.ingestion?.highest_ledger?.toLocaleString() ?? "n/a"}</dd>
              </div>
            </dl>

            {leaderboard.me ? (
              <p>
                <strong>Your Rank:</strong> #{leaderboard.me.rank} (
                {leaderboard.me.score.toLocaleString()} pts)
              </p>
            ) : findAddress ? (
              <p className="leaderboard-note">Address not ranked in this window.</p>
            ) : null}

            {hasHistoricalData ? (
              <div className="leaderboard-empty-window-hint">
                <p className="leaderboard-note">
                  No proved runs landed in this short window. Historical rankings still exist.
                </p>
                <button
                  type="button"
                  onClick={() => {
                    setWindowKey("all");
                    setOffset(0);
                  }}
                >
                  Show All-Time
                </button>
              </div>
            ) : null}
          </section>

          <section className="leaderboard-card leaderboard-surface">
            <h2 className="leaderboard-section-title">Rankings</h2>
            <p className="leaderboard-note">
              Rankings show one row per claimant (their best proved run in this window). Minted is
              the token delta minted for that specific submission.
            </p>
            {isEmptyAllTime ? (
              <div className="leaderboard-empty-cta">
                <p>No proved runs yet.</p>
                <p>Play the game and prove your score to appear here.</p>
                <a className="leaderboard-cta-link" href="/">
                  Play Now
                </a>
              </div>
            ) : leaderboard.entries.length === 0 ? (
              <p className="leaderboard-note">No proved runs in this window yet.</p>
            ) : (
              <div className="leaderboard-table-wrap">
                <table className="leaderboard-table" aria-label="Leaderboard rankings">
                  <thead>
                    <tr>
                      <th scope="col">Rank</th>
                      <th scope="col">Player</th>
                      <th scope="col">Score</th>
                      <th scope="col">Frames</th>
                      <th scope="col">Minted (this run)</th>
                      <th scope="col">Seed</th>
                      <th scope="col">Completed</th>
                      <th scope="col">Claim</th>
                    </tr>
                  </thead>
                  <tbody>
                    {leaderboard.entries.map((entry) => (
                      <tr
                        key={entry.jobId}
                        className={
                          leaderboard.me?.claimantAddress === entry.claimantAddress
                            ? "leaderboard-row--me"
                            : ""
                        }
                      >
                        <td className={rankClass(entry.rank)}>#{entry.rank}</td>
                        <td>
                          <div className="leaderboard-player-cell">
                            <a href={`/leaderboard/${entry.claimantAddress}`}>
                              {displayName(entry)}
                            </a>
                            <code>{abbreviateAddress(entry.claimantAddress)}</code>
                            {entry.profile?.linkUrl && isSafeUrl(entry.profile.linkUrl) ? (
                              <a href={entry.profile.linkUrl} target="_blank" rel="noreferrer">
                                Link
                              </a>
                            ) : null}
                          </div>
                        </td>
                        <td className="leaderboard-cell--num">{entry.score.toLocaleString()}</td>
                        <td className="leaderboard-cell--num">{formatMetric(entry.frameCount)}</td>
                        <td className="leaderboard-cell--num">{formatMetric(entry.mintedDelta)}</td>
                        <td className="leaderboard-cell--mono">{formatHex32(entry.seed)}</td>
                        <td>
                          <RelativeTime value={entry.completedAt} />
                        </td>
                        <td>
                          <span className={claimStatusClass(entry.claimStatus)}>
                            {entry.claimStatus}
                          </span>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}

            <div className="leaderboard-actions">
              <button
                type="button"
                onClick={() => setOffset((current) => Math.max(0, current - limit))}
                disabled={leaderboard.pagination.offset === 0 || loading}
              >
                Previous
              </button>
              <button
                type="button"
                onClick={() => {
                  if (leaderboard.pagination.next_offset !== null) {
                    setOffset(leaderboard.pagination.next_offset);
                  }
                }}
                disabled={leaderboard.pagination.next_offset === null || loading}
              >
                Next
              </button>
            </div>
          </section>
        </>
      ) : null}
    </main>
  );
}
