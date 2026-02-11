import { useCallback, useEffect, useMemo, useState } from "react";
import {
  getLeaderboard,
  getLeaderboardPlayer,
  LeaderboardApiError,
  type LeaderboardEntry,
  type LeaderboardPageResponse,
  type LeaderboardPlayerResponse,
  type LeaderboardWindow,
  updateLeaderboardProfile,
} from "./api";
import { formatUtcDateTime } from "../time";
import "./LeaderboardPage.css";

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

export function LeaderboardPage() {
  const pathname = typeof window !== "undefined" ? window.location.pathname : "/leaderboard";
  const playerAddress = useMemo(() => getPlayerAddressFromPath(pathname), [pathname]);

  const [windowKey, setWindowKey] = useState<LeaderboardWindow>("10m");
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

    setSavingProfile(true);
    setProfileSaveError(null);
    setProfileSavedAt(null);

    try {
      const claimantAddress = playerData.player.claimant_address;
      const updated = await updateLeaderboardProfile(claimantAddress, {
        username: toNullableTrimmed(profileUsername),
        linkUrl: toNullableTrimmed(profileLinkUrl),
      });

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

  if (playerAddress) {
    return (
      <main className="leaderboard-shell">
        <header className="leaderboard-header">
          <div>
            <h1>Player</h1>
            <p>Profile, rankings, and recent proved runs.</p>
          </div>
          <a className="leaderboard-navlink" href="/leaderboard">
            Back To Leaderboard
          </a>
        </header>

        {playerLoading ? <p className="leaderboard-note">Loading player...</p> : null}
        {playerError ? <p className="leaderboard-error">{playerError}</p> : null}

        {playerData ? (
          <>
            <section className="leaderboard-card">
              <h2>
                {playerData.player.profile?.username ??
                  abbreviateAddress(playerData.player.claimant_address)}
              </h2>
              <p>
                <strong>Address:</strong> <code>{playerData.player.claimant_address}</code>
              </p>
              {playerData.player.profile?.linkUrl ? (
                <p>
                  <strong>Link:</strong>{" "}
                  <a href={playerData.player.profile.linkUrl} target="_blank" rel="noreferrer">
                    {playerData.player.profile.linkUrl}
                  </a>
                </p>
              ) : null}
              <div className="leaderboard-grid">
                <div>
                  <dt>Total Runs</dt>
                  <dd>{playerData.player.stats.total_runs.toLocaleString()}</dd>
                </div>
                <div>
                  <dt>Best Score</dt>
                  <dd>{playerData.player.stats.best_score.toLocaleString()}</dd>
                </div>
                <div>
                  <dt>Last Played</dt>
                  <dd>
                    {playerData.player.stats.last_played_at
                      ? formatUtcDateTime(playerData.player.stats.last_played_at)
                      : "n/a"}
                  </dd>
                </div>
              </div>
              <div className="leaderboard-grid">
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
              </div>
            </section>

            <section className="leaderboard-card">
              <h3>Edit Profile</h3>
              <p className="leaderboard-note">
                Updates are currently tied to the claimant address header and intended for
                connected-wallet flows.
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
                    Saved {formatUtcDateTime(profileSavedAt)}
                  </span>
                ) : null}
              </div>
              {profileSaveError ? <p className="leaderboard-error">{profileSaveError}</p> : null}
            </section>

            <section className="leaderboard-card">
              <h3>Recent Runs</h3>
              {playerData.player.recent_runs.length === 0 ? (
                <p className="leaderboard-note">No proved runs yet.</p>
              ) : (
                <div className="leaderboard-table-wrap">
                  <table className="leaderboard-table">
                    <thead>
                      <tr>
                        <th>Score</th>
                        <th>Seed</th>
                        <th>Frames</th>
                        <th>Completed (UTC)</th>
                        <th>Claim</th>
                      </tr>
                    </thead>
                    <tbody>
                      {playerData.player.recent_runs.map((run) => (
                        <tr key={run.jobId}>
                          <td>{run.score.toLocaleString()}</td>
                          <td>{formatHex32(run.seed)}</td>
                          <td>{run.frameCount === null ? "-" : run.frameCount.toLocaleString()}</td>
                          <td>{formatUtcDateTime(run.completedAt)}</td>
                          <td>{run.claimStatus}</td>
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

  return (
    <main className="leaderboard-shell">
      <header className="leaderboard-header">
        <div>
          <h1>Leaderboard</h1>
          <p>Rolling 10m, 24h, and all-time rankings from proved runs.</p>
        </div>
        <a className="leaderboard-navlink" href="/">
          Back To Game
        </a>
      </header>

      <section className="leaderboard-controls">
        <div className="leaderboard-window-buttons">
          {(["10m", "day", "all"] as LeaderboardWindow[]).map((window) => (
            <button
              key={window}
              type="button"
              onClick={() => {
                setWindowKey(window);
                setOffset(0);
              }}
              className={window === windowKey ? "active" : ""}
            >
              {windowLabel(window)}
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
          />
          <button type="submit">Find Me</button>
          <button type="button" onClick={clearFindMe} disabled={!findAddress}>
            Clear
          </button>
        </form>
      </section>

      {loading ? <p className="leaderboard-note">Loading leaderboard...</p> : null}
      {error ? <p className="leaderboard-error">{error}</p> : null}

      {leaderboard ? (
        <>
          <section className="leaderboard-card">
            <p>
              <strong>Window:</strong> {windowLabel(leaderboard.window)}
              {" · "}
              <strong>Updated:</strong> {formatUtcDateTime(leaderboard.generated_at)}
            </p>
            <p>
              <strong>Showing:</strong> {showingStart}-{showingEnd} of{" "}
              {leaderboard.pagination.total.toLocaleString()} players
            </p>
            {leaderboard.me ? (
              <p>
                <strong>Your Rank:</strong> #{leaderboard.me.rank} (
                {leaderboard.me.score.toLocaleString()} pts)
              </p>
            ) : findAddress ? (
              <p className="leaderboard-note">Address not ranked in this window.</p>
            ) : null}
          </section>

          <section className="leaderboard-card">
            {leaderboard.entries.length === 0 ? (
              <p className="leaderboard-note">No proved runs in this window yet.</p>
            ) : (
              <div className="leaderboard-table-wrap">
                <table className="leaderboard-table">
                  <thead>
                    <tr>
                      <th>Rank</th>
                      <th>Player</th>
                      <th>Score</th>
                      <th>Seed</th>
                      <th>Frames</th>
                      <th>Completed (UTC)</th>
                      <th>Claim</th>
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
                        <td>#{entry.rank}</td>
                        <td>
                          <div className="leaderboard-player-cell">
                            <a href={`/leaderboard/${entry.claimantAddress}`}>
                              {displayName(entry)}
                            </a>
                            <code>{abbreviateAddress(entry.claimantAddress)}</code>
                            {entry.profile?.linkUrl ? (
                              <a href={entry.profile.linkUrl} target="_blank" rel="noreferrer">
                                Link
                              </a>
                            ) : null}
                          </div>
                        </td>
                        <td>{entry.score.toLocaleString()}</td>
                        <td>{formatHex32(entry.seed)}</td>
                        <td>
                          {entry.frameCount === null ? "-" : entry.frameCount.toLocaleString()}
                        </td>
                        <td>{formatUtcDateTime(entry.completedAt)}</td>
                        <td>{entry.claimStatus}</td>
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
