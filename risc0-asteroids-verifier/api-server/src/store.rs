use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    str::FromStr,
    sync::Mutex,
};

use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use crate::{now_unix_s, JobStatus, ProofEnvelope, ProofJob, ProveOptionsSummary};
use host::{accelerator, ReceiptKind};

/// Result of attempting to enqueue a new proof job.
pub enum EnqueueResult {
    /// Job inserted successfully.
    Inserted,
    /// Rejected: a job is already queued or running (single-flight mode).
    ProverBusy,
    /// Rejected: at capacity with no finished jobs to evict.
    AtCapacity(usize),
}

/// SQLite-backed persistent job store.
///
/// Proof results (the `ProofEnvelope` with receipt bytes) are stored as JSON
/// files on disk under `{data_dir}/results/`, NOT in SQLite — receipts can be
/// 1.3 MB+ for composite. SQLite stores job metadata and a `result_path`
/// column pointing to the JSON file.
pub struct JobStore {
    conn: Mutex<Connection>,
    results_dir: PathBuf,
}

impl JobStore {
    /// Open (or create) the SQLite database and results directory.
    ///
    /// On startup, any jobs left as `queued` or `running` from a previous crash
    /// are marked `failed` with error "server restarted". Result files in the
    /// `results/` directory that are not referenced by any DB row are removed.
    pub fn open(data_dir: &Path) -> Result<Self, String> {
        fs::create_dir_all(data_dir)
            .map_err(|e| format!("failed to create data dir {}: {e}", data_dir.display()))?;

        let results_dir = data_dir.join("results");
        fs::create_dir_all(&results_dir)
            .map_err(|e| format!("failed to create results dir: {e}"))?;

        let db_path = data_dir.join("jobs.db");
        let conn = Connection::open(&db_path)
            .map_err(|e| format!("failed to open SQLite at {}: {e}", db_path.display()))?;

        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA busy_timeout=5000;
             PRAGMA synchronous=NORMAL;",
        )
        .map_err(|e| format!("failed to set pragmas: {e}"))?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS jobs (
                job_id              TEXT PRIMARY KEY,
                status              TEXT NOT NULL,
                created_at          INTEGER NOT NULL,
                started_at          INTEGER,
                finished_at         INTEGER,
                tape_size_bytes     INTEGER NOT NULL,
                opt_max_frames        INTEGER NOT NULL,
                opt_receipt_kind      TEXT NOT NULL,
                opt_segment_limit_po2 INTEGER NOT NULL,
                opt_allow_dev_mode    INTEGER NOT NULL,
                opt_verify_receipt    INTEGER NOT NULL,
                opt_accelerator       TEXT NOT NULL,
                result_path         TEXT,
                error               TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_jobs_status ON jobs(status);
            CREATE INDEX IF NOT EXISTS idx_jobs_finished_at ON jobs(finished_at);",
        )
        .map_err(|e| format!("failed to create schema: {e}"))?;

        // Idempotent migration: add error_code column if it doesn't exist.
        conn.execute("ALTER TABLE jobs ADD COLUMN error_code TEXT", [])
            .ok();

        let store = Self {
            conn: Mutex::new(conn),
            results_dir,
        };

        let recovered = store.recover_on_startup()?;
        if recovered > 0 {
            tracing::warn!(recovered, "marked orphaned queued/running jobs as failed");
        }

        let orphans = store.cleanup_orphan_results()?;
        if orphans > 0 {
            tracing::warn!(orphans, "removed orphaned result files");
        }

        Ok(store)
    }

    /// Mark any queued/running jobs from a previous crash as failed.
    fn recover_on_startup(&self) -> Result<usize, String> {
        let conn = self.conn.lock().unwrap();
        let now = now_unix_s();
        conn.execute(
            "UPDATE jobs SET status = 'failed', finished_at = ?1, error = 'server restarted',
                    error_code = 'server_restarted'
             WHERE status IN ('queued', 'running')",
            params![now as i64],
        )
        .map_err(|e| format!("recover_on_startup failed: {e}"))
    }

    /// Extract a safe result filename from the DB value.
    ///
    /// Accepts both current values (`{job_id}.json`) and legacy absolute paths.
    fn result_filename_from_db_value(value: &str) -> Result<String, String> {
        let filename = Path::new(value)
            .file_name()
            .ok_or_else(|| format!("invalid result_path without filename: {value}"))?
            .to_string_lossy()
            .into_owned();

        if filename.is_empty() {
            return Err("invalid result_path with empty filename".to_string());
        }
        if !filename.ends_with(".json") {
            return Err(format!(
                "invalid result_path extension (expected .json): {value}"
            ));
        }

        Ok(filename)
    }

    fn result_path_from_db_value(&self, value: &str) -> Result<PathBuf, String> {
        let filename = Self::result_filename_from_db_value(value)?;
        Ok(self.results_dir.join(filename))
    }

    /// Best-effort cleanup of a stored result file path.
    fn remove_result_file_best_effort(&self, stored_path: &str) {
        match self.result_path_from_db_value(stored_path) {
            Ok(path) => {
                let _ = fs::remove_file(path);
            }
            Err(e) => tracing::warn!("skipping result file cleanup for invalid path: {e}"),
        }
    }

    /// Remove result files from disk that are not referenced by any DB row.
    ///
    /// Catches files orphaned by a crash between `fs::write` in `complete()`
    /// and the subsequent SQLite UPDATE, as well as files left behind by any
    /// cleanup path (`delete`, `sweep`, `try_enqueue` eviction) that silently
    /// failed to remove the file on disk.
    fn cleanup_orphan_results(&self) -> Result<usize, String> {
        let referenced: HashSet<String> = {
            let conn = self.conn.lock().unwrap();
            let mut stmt = conn
                .prepare("SELECT result_path FROM jobs WHERE result_path IS NOT NULL")
                .map_err(|e| format!("orphan cleanup: query failed: {e}"))?;
            let mut set = HashSet::new();
            let rows = stmt
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(|e| format!("orphan cleanup: iteration failed: {e}"))?;

            for row in rows {
                let stored_path =
                    row.map_err(|e| format!("orphan cleanup: row decode failed: {e}"))?;
                let filename = match Self::result_filename_from_db_value(&stored_path) {
                    Ok(value) => value,
                    Err(e) => {
                        tracing::warn!(
                            "orphan cleanup disabled due to invalid result_path value: {e}"
                        );
                        return Ok(0);
                    }
                };
                set.insert(filename);
            }

            set
        };

        let entries = match fs::read_dir(&self.results_dir) {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("orphan cleanup: failed to read results dir: {e}");
                return Ok(0);
            }
        };

        let mut removed = 0usize;
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.ends_with(".json") && !referenced.contains(name_str.as_ref()) {
                match fs::remove_file(entry.path()) {
                    Ok(()) => {
                        tracing::info!(file = %name_str, "removed orphaned result file");
                        removed += 1;
                    }
                    Err(e) => {
                        tracing::warn!(file = %name_str, "failed to remove orphaned result file: {e}");
                    }
                }
            }
        }

        Ok(removed)
    }

    #[cfg(test)]
    pub fn insert(&self, job: &ProofJob) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        Self::insert_row(&conn, job)
    }

    /// Atomically check single-flight + capacity constraints, evict if needed,
    /// and insert the new job — all under a single mutex acquisition.
    ///
    /// This eliminates the TOCTOU race where two concurrent requests could both
    /// pass `has_active_job()` and both insert before either enters `running`.
    pub fn try_enqueue(&self, job: &ProofJob, max_jobs: usize) -> Result<EnqueueResult, String> {
        let evicted_path = {
            let conn = self.conn.lock().unwrap();

            // 1. Reject if a job is already active (single-flight guard).
            let active: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM jobs WHERE status IN ('queued', 'running')",
                    [],
                    |row| row.get(0),
                )
                .map_err(|e| format!("try_enqueue active check: {e}"))?;
            if active > 0 {
                return Ok(EnqueueResult::ProverBusy);
            }

            // 2. Evict oldest finished job if at capacity.
            let count: i64 = conn
                .query_row("SELECT COUNT(*) FROM jobs", [], |row| row.get(0))
                .map_err(|e| format!("try_enqueue count: {e}"))?;

            let mut evicted_path: Option<String> = None;
            if count as usize >= max_jobs {
                let row: Option<(String, Option<String>)> = conn
                    .query_row(
                        "SELECT job_id, result_path FROM jobs
                         WHERE status IN ('succeeded', 'failed')
                         ORDER BY COALESCE(finished_at, created_at) ASC
                         LIMIT 1",
                        [],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .optional()
                    .map_err(|e| format!("try_enqueue evict lookup: {e}"))?;

                match row {
                    Some((evict_id, result_path)) => {
                        conn.execute("DELETE FROM jobs WHERE job_id = ?1", params![evict_id])
                            .map_err(|e| format!("try_enqueue evict delete: {e}"))?;
                        evicted_path = result_path;
                        tracing::info!(evicted_job_id = %evict_id, "evicted oldest finished job to make room");
                    }
                    None => return Ok(EnqueueResult::AtCapacity(max_jobs)),
                }
            }

            // 3. Insert the new job.
            Self::insert_row(&conn, job)?;

            evicted_path
        };

        // File cleanup outside the lock.
        if let Some(ref path) = evicted_path {
            self.remove_result_file_best_effort(path);
        }

        Ok(EnqueueResult::Inserted)
    }

    fn insert_row(conn: &Connection, job: &ProofJob) -> Result<(), String> {
        conn.execute(
            "INSERT INTO jobs (
                job_id, status, created_at, started_at, finished_at,
                tape_size_bytes,
                opt_max_frames, opt_receipt_kind, opt_segment_limit_po2,
                opt_allow_dev_mode, opt_verify_receipt, opt_accelerator,
                result_path, error, error_code
            ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
            params![
                job.job_id.to_string(),
                status_to_str(job.status),
                job.created_at_unix_s as i64,
                job.started_at_unix_s.map(|v| v as i64),
                job.finished_at_unix_s.map(|v| v as i64),
                job.tape_size_bytes as i64,
                job.options.max_frames as i64,
                job.options.receipt_kind.as_str(),
                job.options.segment_limit_po2 as i64,
                job.options.allow_dev_mode as i64,
                job.options.verify_receipt as i64,
                job.options.accelerator,
                Option::<String>::None,
                job.error.as_deref(),
                job.error_code.as_deref(),
            ],
        )
        .map_err(|e| format!("insert job failed: {e}"))?;
        Ok(())
    }

    /// Read a job by ID, loading the result JSON from disk if available.
    pub fn get(&self, job_id: Uuid) -> Result<Option<ProofJob>, String> {
        // Scope the mutex so it is released before file I/O in row_to_job().
        let row = {
            let conn = self.conn.lock().unwrap();
            conn.query_row(
                "SELECT job_id, status, created_at, started_at, finished_at,
                        tape_size_bytes,
                        opt_max_frames, opt_receipt_kind, opt_segment_limit_po2,
                        opt_allow_dev_mode, opt_verify_receipt, opt_accelerator,
                        result_path, error, error_code
                 FROM jobs WHERE job_id = ?1",
                params![job_id.to_string()],
                |row| {
                    Ok(RawJobRow {
                        job_id: row.get(0)?,
                        status: row.get(1)?,
                        created_at: row.get(2)?,
                        started_at: row.get(3)?,
                        finished_at: row.get(4)?,
                        tape_size_bytes: row.get(5)?,
                        opt_max_frames: row.get(6)?,
                        opt_receipt_kind: row.get(7)?,
                        opt_segment_limit_po2: row.get(8)?,
                        opt_allow_dev_mode: row.get(9)?,
                        opt_verify_receipt: row.get(10)?,
                        _opt_accelerator: row.get(11)?,
                        result_path: row.get(12)?,
                        error: row.get(13)?,
                        error_code: row.get(14)?,
                    })
                },
            )
            .optional()
            .map_err(|e| format!("get job failed: {e}"))?
        };

        match row {
            Some(r) => Ok(Some(self.row_to_job(r)?)),
            None => Ok(None),
        }
    }

    pub fn update_status(
        &self,
        job_id: Uuid,
        status: JobStatus,
        started_at: Option<u64>,
    ) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE jobs SET status = ?1, started_at = COALESCE(?2, started_at) WHERE job_id = ?3",
            params![
                status_to_str(status),
                started_at.map(|v| v as i64),
                job_id.to_string()
            ],
        )
        .map_err(|e| format!("update_status failed: {e}"))?;
        Ok(())
    }

    /// Mark a job as succeeded and write the result envelope to disk as JSON.
    pub fn complete(&self, job_id: Uuid, result: ProofEnvelope) -> Result<(), String> {
        let filename = format!("{job_id}.json");
        let result_path = self.results_dir.join(&filename);
        let json =
            serde_json::to_vec(&result).map_err(|e| format!("failed to serialize result: {e}"))?;
        fs::write(&result_path, json).map_err(|e| format!("failed to write result file: {e}"))?;

        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE jobs SET status = 'succeeded', finished_at = ?1, result_path = ?2,
                    error = NULL, error_code = NULL
             WHERE job_id = ?3",
            params![now_unix_s() as i64, filename, job_id.to_string()],
        )
        .map_err(|e| format!("complete job failed: {e}"))?;
        Ok(())
    }

    pub fn fail(&self, job_id: Uuid, error: String, error_code: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE jobs SET status = 'failed', finished_at = ?1, error = ?2,
                    error_code = ?3 WHERE job_id = ?4",
            params![now_unix_s() as i64, error, error_code, job_id.to_string()],
        )
        .map_err(|e| format!("fail job failed: {e}"))?;
        Ok(())
    }

    /// Delete a job and its result file. Returns true if the row existed.
    pub fn delete(&self, job_id: Uuid) -> Result<bool, String> {
        let (deleted, result_path) = {
            let conn = self.conn.lock().unwrap();
            let result_path: Option<String> = conn
                .query_row(
                    "SELECT result_path FROM jobs WHERE job_id = ?1",
                    params![job_id.to_string()],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| format!("delete lookup failed: {e}"))?
                .flatten();

            let deleted = conn
                .execute(
                    "DELETE FROM jobs WHERE job_id = ?1",
                    params![job_id.to_string()],
                )
                .map_err(|e| format!("delete job failed: {e}"))?;

            (deleted, result_path)
        };

        // Clean up result file outside the lock.
        if deleted > 0 {
            if let Some(ref path) = result_path {
                self.remove_result_file_best_effort(path);
            }
        }

        Ok(deleted > 0)
    }

    #[cfg(test)]
    pub fn has_active_job(&self) -> Result<bool, String> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM jobs WHERE status IN ('queued', 'running')",
                [],
                |row| row.get(0),
            )
            .map_err(|e| format!("has_active_job failed: {e}"))?;
        Ok(count > 0)
    }

    #[cfg(test)]
    pub fn count(&self) -> Result<usize, String> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM jobs", [], |row| row.get(0))
            .map_err(|e| format!("count failed: {e}"))?;
        Ok(count as usize)
    }

    /// Returns (queued, running, total).
    pub fn count_by_status(&self) -> Result<(usize, usize, usize), String> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT
                 COUNT(*) FILTER (WHERE status = 'queued'),
                 COUNT(*) FILTER (WHERE status = 'running'),
                 COUNT(*)
             FROM jobs",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)? as usize,
                    row.get::<_, i64>(1)? as usize,
                    row.get::<_, i64>(2)? as usize,
                ))
            },
        )
        .map_err(|e| format!("count_by_status failed: {e}"))
    }

    /// Evict the oldest finished (succeeded/failed) job. Returns the evicted ID.
    #[cfg(test)]
    pub fn evict_oldest_finished(&self) -> Result<Option<Uuid>, String> {
        let conn = self.conn.lock().unwrap();
        let row: Option<(String, Option<String>)> = conn
            .query_row(
                "SELECT job_id, result_path FROM jobs
                 WHERE status IN ('succeeded', 'failed')
                 ORDER BY COALESCE(finished_at, created_at) ASC
                 LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|e| format!("evict lookup failed: {e}"))?;

        let Some((id_str, result_path)) = row else {
            return Ok(None);
        };

        if let Some(ref path) = result_path {
            self.remove_result_file_best_effort(path);
        }

        conn.execute("DELETE FROM jobs WHERE job_id = ?1", params![id_str])
            .map_err(|e| format!("evict delete failed: {e}"))?;

        let uuid = Uuid::parse_str(&id_str).map_err(|e| format!("bad uuid in db: {e}"))?;
        Ok(Some(uuid))
    }

    /// Sweep expired and stuck jobs. Returns the number of jobs reaped.
    pub fn sweep(&self, ttl_secs: u64, running_timeout_secs: u64) -> Result<usize, String> {
        let now = now_unix_s() as i64;
        let ttl_cutoff = now.saturating_sub(ttl_secs as i64);
        let running_cutoff = now.saturating_sub(running_timeout_secs as i64);

        let sweep_where =
            "(status IN ('running', 'queued') AND COALESCE(started_at, created_at) < ?1)
             OR
             (status IN ('succeeded', 'failed') AND COALESCE(finished_at, created_at) < ?2)";

        // Collect result files to delete before removing rows. Release lock
        // between SELECT and file I/O so other operations aren't blocked.
        let to_reap: Vec<(String, Option<String>)> = {
            let conn = self.conn.lock().unwrap();
            let mut stmt = conn
                .prepare(&format!(
                    "SELECT job_id, result_path FROM jobs WHERE {sweep_where}"
                ))
                .map_err(|e| format!("sweep select failed: {e}"))?;

            let rows: Vec<_> = stmt
                .query_map(params![running_cutoff, ttl_cutoff], |row| {
                    Ok((row.get(0)?, row.get(1)?))
                })
                .map_err(|e| format!("sweep query failed: {e}"))?
                .filter_map(|r| r.ok())
                .collect();
            rows
        };

        if to_reap.is_empty() {
            return Ok(0);
        }

        for (ref id, ref result_path) in &to_reap {
            if let Some(ref path) = result_path {
                self.remove_result_file_best_effort(path);
            }
            tracing::info!(job_id = %id, "sweeping expired job");
        }

        // Delete with the same parameterized WHERE clause (no string interpolation).
        let conn = self.conn.lock().unwrap();
        let deleted = conn
            .execute(
                &format!("DELETE FROM jobs WHERE {sweep_where}"),
                params![running_cutoff, ttl_cutoff],
            )
            .map_err(|e| format!("sweep delete failed: {e}"))?;

        Ok(deleted)
    }

    // ── internal helpers ──

    fn row_to_job(&self, r: RawJobRow) -> Result<ProofJob, String> {
        let job_id = Uuid::parse_str(&r.job_id).map_err(|e| format!("bad uuid in db: {e}"))?;
        let status = status_from_str(&r.status)?;
        let receipt_kind = ReceiptKind::from_str(&r.opt_receipt_kind)
            .map_err(|e| format!("bad receipt_kind in db: {e}"))?;

        let result = if status == JobStatus::Succeeded {
            if let Some(ref path) = r.result_path {
                match self.result_path_from_db_value(path) {
                    Ok(resolved) => match fs::read(&resolved) {
                        Ok(bytes) => {
                            let envelope: ProofEnvelope =
                                serde_json::from_slice(&bytes).map_err(|e| {
                                    format!(
                                        "failed to deserialize result {}: {e}",
                                        resolved.display()
                                    )
                                })?;
                            Some(envelope)
                        }
                        Err(e) => {
                            tracing::warn!(
                                job_id = %job_id,
                                path = %resolved.display(),
                                "result file missing: {e}"
                            );
                            None
                        }
                    },
                    Err(e) => {
                        tracing::warn!(
                            job_id = %job_id,
                            stored_path = %path,
                            "invalid result file path: {e}"
                        );
                        None
                    }
                }
            } else {
                None
            }
        } else {
            None
        };

        Ok(ProofJob {
            job_id,
            status,
            created_at_unix_s: r.created_at as u64,
            started_at_unix_s: r.started_at.map(|v| v as u64),
            finished_at_unix_s: r.finished_at.map(|v| v as u64),
            tape_size_bytes: r.tape_size_bytes as usize,
            options: ProveOptionsSummary {
                max_frames: r.opt_max_frames as u32,
                receipt_kind,
                segment_limit_po2: r.opt_segment_limit_po2 as u32,
                allow_dev_mode: r.opt_allow_dev_mode != 0,
                verify_receipt: r.opt_verify_receipt != 0,
                accelerator: accelerator(),
            },
            result,
            error: r.error,
            error_code: r.error_code,
        })
    }
}

struct RawJobRow {
    job_id: String,
    status: String,
    created_at: i64,
    started_at: Option<i64>,
    finished_at: Option<i64>,
    tape_size_bytes: i64,
    opt_max_frames: i64,
    opt_receipt_kind: String,
    opt_segment_limit_po2: i64,
    opt_allow_dev_mode: i64,
    opt_verify_receipt: i64,
    _opt_accelerator: String,
    result_path: Option<String>,
    error: Option<String>,
    error_code: Option<String>,
}

fn status_to_str(status: JobStatus) -> &'static str {
    match status {
        JobStatus::Queued => "queued",
        JobStatus::Running => "running",
        JobStatus::Succeeded => "succeeded",
        JobStatus::Failed => "failed",
    }
}

fn status_from_str(s: &str) -> Result<JobStatus, String> {
    match s {
        "queued" => Ok(JobStatus::Queued),
        "running" => Ok(JobStatus::Running),
        "succeeded" => Ok(JobStatus::Succeeded),
        "failed" => Ok(JobStatus::Failed),
        _ => Err(format!("unknown job status in db: {s}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options_summary;
    use host::ProveOptions;
    use tempfile::TempDir;

    fn test_store() -> (JobStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = JobStore::open(dir.path()).unwrap();
        (store, dir)
    }

    fn sample_job() -> ProofJob {
        ProofJob {
            job_id: Uuid::new_v4(),
            status: JobStatus::Queued,
            created_at_unix_s: now_unix_s(),
            started_at_unix_s: None,
            finished_at_unix_s: None,
            tape_size_bytes: 100,
            options: options_summary(ProveOptions::default()),
            result: None,
            error: None,
            error_code: None,
        }
    }

    #[test]
    fn insert_and_get() {
        let (store, _dir) = test_store();
        let job = sample_job();
        store.insert(&job).unwrap();

        let loaded = store.get(job.job_id).unwrap().unwrap();
        assert_eq!(loaded.job_id, job.job_id);
        assert_eq!(loaded.status, JobStatus::Queued);
        assert_eq!(loaded.tape_size_bytes, 100);
    }

    #[test]
    fn get_missing_returns_none() {
        let (store, _dir) = test_store();
        assert!(store.get(Uuid::new_v4()).unwrap().is_none());
    }

    #[test]
    fn update_status_to_running() {
        let (store, _dir) = test_store();
        let job = sample_job();
        store.insert(&job).unwrap();

        let now = now_unix_s();
        store
            .update_status(job.job_id, JobStatus::Running, Some(now))
            .unwrap();

        let loaded = store.get(job.job_id).unwrap().unwrap();
        assert_eq!(loaded.status, JobStatus::Running);
        assert_eq!(loaded.started_at_unix_s, Some(now));
    }

    #[test]
    fn fail_sets_error() {
        let (store, _dir) = test_store();
        let job = sample_job();
        store.insert(&job).unwrap();

        store
            .fail(job.job_id, "something broke".to_string(), "proof_error")
            .unwrap();

        let loaded = store.get(job.job_id).unwrap().unwrap();
        assert_eq!(loaded.status, JobStatus::Failed);
        assert_eq!(loaded.error.as_deref(), Some("something broke"));
    }

    #[test]
    fn delete_removes_job() {
        let (store, _dir) = test_store();
        let job = sample_job();
        store.insert(&job).unwrap();

        assert!(store.delete(job.job_id).unwrap());
        assert!(store.get(job.job_id).unwrap().is_none());
        // Deleting again returns false.
        assert!(!store.delete(job.job_id).unwrap());
    }

    #[test]
    fn has_active_job_detects_queued_and_running() {
        let (store, _dir) = test_store();
        assert!(!store.has_active_job().unwrap());

        let job = sample_job();
        store.insert(&job).unwrap();
        assert!(store.has_active_job().unwrap());

        store
            .update_status(job.job_id, JobStatus::Running, Some(now_unix_s()))
            .unwrap();
        assert!(store.has_active_job().unwrap());

        store
            .fail(job.job_id, "done".to_string(), "proof_error")
            .unwrap();
        assert!(!store.has_active_job().unwrap());
    }

    #[test]
    fn count_and_count_by_status() {
        let (store, _dir) = test_store();
        assert_eq!(store.count().unwrap(), 0);

        let j1 = sample_job();
        let j2 = sample_job();
        store.insert(&j1).unwrap();
        store.insert(&j2).unwrap();
        assert_eq!(store.count().unwrap(), 2);

        store
            .update_status(j1.job_id, JobStatus::Running, Some(now_unix_s()))
            .unwrap();
        let (queued, running, total) = store.count_by_status().unwrap();
        assert_eq!(queued, 1);
        assert_eq!(running, 1);
        assert_eq!(total, 2);
    }

    #[test]
    fn evict_oldest_finished() {
        let (store, _dir) = test_store();

        let mut j1 = sample_job();
        j1.created_at_unix_s = 1000;
        store.insert(&j1).unwrap();
        store
            .fail(j1.job_id, "old".to_string(), "proof_error")
            .unwrap();

        let mut j2 = sample_job();
        j2.created_at_unix_s = 2000;
        store.insert(&j2).unwrap();
        store
            .fail(j2.job_id, "newer".to_string(), "proof_error")
            .unwrap();

        let evicted = store.evict_oldest_finished().unwrap();
        assert_eq!(evicted, Some(j1.job_id));
        assert_eq!(store.count().unwrap(), 1);
    }

    #[test]
    fn try_enqueue_rejects_when_active() {
        let (store, _dir) = test_store();

        let j1 = sample_job();
        assert!(matches!(
            store.try_enqueue(&j1, 64).unwrap(),
            EnqueueResult::Inserted
        ));

        // Second enqueue should be rejected — j1 is still queued.
        let j2 = sample_job();
        assert!(matches!(
            store.try_enqueue(&j2, 64).unwrap(),
            EnqueueResult::ProverBusy
        ));
        assert_eq!(store.count().unwrap(), 1);

        // Finish j1, then j2 should be accepted.
        store.fail(j1.job_id, "done".into(), "proof_error").unwrap();
        assert!(matches!(
            store.try_enqueue(&j2, 64).unwrap(),
            EnqueueResult::Inserted
        ));
        assert_eq!(store.count().unwrap(), 2);
    }

    #[test]
    fn try_enqueue_evicts_at_capacity() {
        let (store, _dir) = test_store();
        let max_jobs = 3;

        // Fill to capacity with finished jobs.
        for i in 0..max_jobs {
            let mut j = sample_job();
            j.created_at_unix_s = 1000 + i as u64;
            store.insert(&j).unwrap();
            store
                .fail(j.job_id, format!("err-{i}"), "proof_error")
                .unwrap();
        }
        assert_eq!(store.count().unwrap(), max_jobs);

        // Enqueue should evict the oldest and insert.
        let new_job = sample_job();
        assert!(matches!(
            store.try_enqueue(&new_job, max_jobs).unwrap(),
            EnqueueResult::Inserted
        ));
        // Count stays at max_jobs (evicted one, inserted one).
        assert_eq!(store.count().unwrap(), max_jobs);
    }

    #[test]
    fn try_enqueue_at_capacity_all_active_returns_at_capacity() {
        let (store, _dir) = test_store();

        // Insert one active job — capacity of 1.
        let j1 = sample_job();
        store.insert(&j1).unwrap();

        // At capacity=1 with the active job, no finished jobs to evict →
        // single-flight guard fires first (ProverBusy, not AtCapacity).
        let j2 = sample_job();
        assert!(matches!(
            store.try_enqueue(&j2, 1).unwrap(),
            EnqueueResult::ProverBusy
        ));
    }

    #[test]
    fn recover_on_startup_marks_orphans_failed() {
        let dir = TempDir::new().unwrap();
        let job_id;

        // First open — insert a "running" job, then drop the store (simulating crash).
        {
            let store = JobStore::open(dir.path()).unwrap();
            let mut job = sample_job();
            job.status = JobStatus::Running;
            job.started_at_unix_s = Some(now_unix_s());
            job_id = job.job_id;
            store.insert(&job).unwrap();
            store
                .update_status(job.job_id, JobStatus::Running, Some(now_unix_s()))
                .unwrap();
        }

        // Re-open — recovery should mark it failed with error_code.
        let store = JobStore::open(dir.path()).unwrap();
        let (_, running, _) = store.count_by_status().unwrap();
        assert_eq!(running, 0);

        let loaded = store.get(job_id).unwrap().unwrap();
        assert_eq!(loaded.status, JobStatus::Failed);
        assert_eq!(loaded.error.as_deref(), Some("server restarted"));
        assert_eq!(loaded.error_code.as_deref(), Some("server_restarted"));
    }

    #[test]
    fn fail_with_code_and_get() {
        let (store, _dir) = test_store();
        let job = sample_job();
        store.insert(&job).unwrap();

        store
            .fail(
                job.job_id,
                "proof generation timed out".to_string(),
                "proof_timeout",
            )
            .unwrap();

        let loaded = store.get(job.job_id).unwrap().unwrap();
        assert_eq!(loaded.status, JobStatus::Failed);
        assert_eq!(loaded.error.as_deref(), Some("proof generation timed out"));
        assert_eq!(loaded.error_code.as_deref(), Some("proof_timeout"));
    }

    #[test]
    fn open_succeeds_on_invalid_result_path_value() {
        let dir = TempDir::new().unwrap();
        let results_dir = dir.path().join("results");
        let orphan = results_dir.join(format!("{}.json", Uuid::new_v4()));

        {
            let store = JobStore::open(dir.path()).unwrap();
            let job = sample_job();
            store.insert(&job).unwrap();

            let conn = store.conn.lock().unwrap();
            conn.execute(
                "UPDATE jobs SET status = 'succeeded', result_path = ?1 WHERE job_id = ?2",
                params!["not-a-json-result", job.job_id.to_string()],
            )
            .unwrap();

            fs::write(&orphan, b"{}").unwrap();
        }

        let reopened = JobStore::open(dir.path());
        assert!(
            reopened.is_ok(),
            "startup should not fail on invalid result_path"
        );
        assert!(
            orphan.exists(),
            "cleanup should be skipped when DB contains invalid result_path values"
        );
    }

    #[test]
    fn delete_never_removes_files_outside_results_dir() {
        let dir = TempDir::new().unwrap();
        let outside_file = dir.path().join("outside-delete.json");
        fs::write(&outside_file, b"keep").unwrap();

        let store = JobStore::open(dir.path()).unwrap();
        let job = sample_job();
        store.insert(&job).unwrap();

        {
            let conn = store.conn.lock().unwrap();
            conn.execute(
                "UPDATE jobs SET status = 'failed', result_path = ?1 WHERE job_id = ?2",
                params![
                    outside_file.to_string_lossy().as_ref(),
                    job.job_id.to_string()
                ],
            )
            .unwrap();
        }

        assert!(store.delete(job.job_id).unwrap());
        assert!(
            outside_file.exists(),
            "delete should not remove files outside results_dir"
        );
    }

    #[test]
    fn cleanup_removes_orphaned_result_files() {
        let dir = TempDir::new().unwrap();
        let results_dir = dir.path().join("results");

        // First open to create schema and results dir.
        {
            let _store = JobStore::open(dir.path()).unwrap();
        }

        // Simulate crash mid-complete(): file exists but no DB row references it.
        let orphan = results_dir.join(format!("{}.json", Uuid::new_v4()));
        fs::write(&orphan, b"{}").unwrap();
        assert!(orphan.exists());

        // Re-open triggers orphan cleanup.
        let _store = JobStore::open(dir.path()).unwrap();
        assert!(
            !orphan.exists(),
            "orphaned result file should have been removed"
        );
    }

    #[test]
    fn cleanup_preserves_referenced_and_removes_orphans() {
        let dir = TempDir::new().unwrap();
        let results_dir = dir.path().join("results");

        let (kept, orphan);
        {
            let store = JobStore::open(dir.path()).unwrap();
            let job = sample_job();
            store.insert(&job).unwrap();

            // Simulate a completed job: write result file and update DB to reference it.
            kept = results_dir.join(format!("{}.json", job.job_id));
            fs::write(&kept, b"{}").unwrap();
            {
                let conn = store.conn.lock().unwrap();
                conn.execute(
                    "UPDATE jobs SET status = 'succeeded', finished_at = ?1,
                            result_path = ?2, error = NULL
                     WHERE job_id = ?3",
                    params![
                        now_unix_s() as i64,
                        kept.to_string_lossy().as_ref(),
                        job.job_id.to_string()
                    ],
                )
                .unwrap();
            }

            // Create an orphan alongside the valid file.
            orphan = results_dir.join(format!("{}.json", Uuid::new_v4()));
            fs::write(&orphan, b"{}").unwrap();
        }

        // Re-open triggers orphan cleanup.
        let _store = JobStore::open(dir.path()).unwrap();
        assert!(kept.exists(), "referenced result file should be preserved");
        assert!(
            !orphan.exists(),
            "orphaned result file should have been removed"
        );
    }

    #[test]
    fn sweep_empty_store_returns_zero() {
        let (store, _dir) = test_store();
        assert_eq!(store.sweep(1, 1).unwrap(), 0);
    }

    #[test]
    fn sweep_reaps_expired_finished_jobs() {
        let (store, _dir) = test_store();

        // Expired job: failed with very old finished_at.
        let j1 = sample_job();
        store.insert(&j1).unwrap();
        store.fail(j1.job_id, "old".into(), "proof_error").unwrap();
        {
            let conn = store.conn.lock().unwrap();
            conn.execute(
                "UPDATE jobs SET finished_at = 1000 WHERE job_id = ?1",
                params![j1.job_id.to_string()],
            )
            .unwrap();
        }

        // Fresh job: failed just now — should survive.
        let j2 = sample_job();
        store.insert(&j2).unwrap();
        store
            .fail(j2.job_id, "fresh".into(), "proof_error")
            .unwrap();

        // TTL=1 → cutoff ≈ now-1. finished_at=1000 is way before cutoff.
        // Large running_timeout so we don't accidentally reap running jobs.
        let reaped = store.sweep(1, 86400).unwrap();
        assert_eq!(reaped, 1);
        assert!(
            store.get(j1.job_id).unwrap().is_none(),
            "expired job should be reaped"
        );
        assert!(
            store.get(j2.job_id).unwrap().is_some(),
            "fresh job should survive"
        );
    }

    #[test]
    fn sweep_reaps_stuck_running_jobs() {
        let (store, _dir) = test_store();

        let job = sample_job();
        store.insert(&job).unwrap();
        // Set started_at to ancient time to simulate a stuck job.
        store
            .update_status(job.job_id, JobStatus::Running, Some(1000))
            .unwrap();

        // running_timeout=1 → cutoff ≈ now-1. started_at=1000 is way before.
        // Large TTL so we don't accidentally reap finished jobs.
        let reaped = store.sweep(86400, 1).unwrap();
        assert_eq!(reaped, 1);
        assert!(store.get(job.job_id).unwrap().is_none());
    }

    #[test]
    fn sweep_deletes_result_files() {
        let (store, dir) = test_store();
        let results_dir = dir.path().join("results");

        let job = sample_job();
        store.insert(&job).unwrap();

        // Simulate a succeeded job with a result file, backdated.
        let result_file = results_dir.join(format!("{}.json", job.job_id));
        fs::write(&result_file, b"{}").unwrap();
        {
            let conn = store.conn.lock().unwrap();
            conn.execute(
                "UPDATE jobs SET status = 'succeeded', finished_at = 1000,
                        result_path = ?1, error = NULL
                 WHERE job_id = ?2",
                params![
                    result_file.to_string_lossy().as_ref(),
                    job.job_id.to_string()
                ],
            )
            .unwrap();
        }

        assert!(result_file.exists());
        let reaped = store.sweep(1, 86400).unwrap();
        assert_eq!(reaped, 1);
        assert!(
            !result_file.exists(),
            "sweep should delete result file of reaped job"
        );
    }

    #[test]
    fn delete_removes_result_file() {
        let (store, dir) = test_store();
        let results_dir = dir.path().join("results");

        let job = sample_job();
        store.insert(&job).unwrap();

        // Simulate a succeeded job with a result file.
        let result_file = results_dir.join(format!("{}.json", job.job_id));
        fs::write(&result_file, b"{}").unwrap();
        {
            let conn = store.conn.lock().unwrap();
            conn.execute(
                "UPDATE jobs SET status = 'succeeded', finished_at = ?1,
                        result_path = ?2, error = NULL
                 WHERE job_id = ?3",
                params![
                    now_unix_s() as i64,
                    result_file.to_string_lossy().as_ref(),
                    job.job_id.to_string()
                ],
            )
            .unwrap();
        }

        assert!(result_file.exists());
        assert!(store.delete(job.job_id).unwrap());
        assert!(
            !result_file.exists(),
            "delete should remove result file from disk"
        );
    }
}
