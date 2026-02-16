mod db;
mod files;
#[cfg(test)]
mod tests;

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use crate::{now_unix_s, JobStatus, ProofEnvelope, ProofJob, ProveOptionsSummary};
use db::status_to_str;

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

const JOBS_SCHEMA_MIGRATIONS: [(&str, &str); 9] = [
    (
        "opt_max_frames",
        "ALTER TABLE jobs ADD COLUMN opt_max_frames INTEGER NOT NULL DEFAULT 18000;",
    ),
    (
        "opt_receipt_kind",
        "ALTER TABLE jobs ADD COLUMN opt_receipt_kind TEXT NOT NULL DEFAULT 'composite';",
    ),
    (
        "opt_segment_limit_po2",
        "ALTER TABLE jobs ADD COLUMN opt_segment_limit_po2 INTEGER NOT NULL DEFAULT 21;",
    ),
    (
        "opt_proof_mode",
        "ALTER TABLE jobs ADD COLUMN opt_proof_mode TEXT NOT NULL DEFAULT 'secure';",
    ),
    (
        "opt_verify_mode",
        "ALTER TABLE jobs ADD COLUMN opt_verify_mode TEXT NOT NULL DEFAULT 'verify';",
    ),
    (
        "opt_accelerator",
        "ALTER TABLE jobs ADD COLUMN opt_accelerator TEXT NOT NULL DEFAULT 'cpu';",
    ),
    (
        "result_path",
        "ALTER TABLE jobs ADD COLUMN result_path TEXT;",
    ),
    (
        "error",
        "ALTER TABLE jobs ADD COLUMN error TEXT;",
    ),
    (
        "error_code",
        "ALTER TABLE jobs ADD COLUMN error_code TEXT;",
    ),
];

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
                 opt_proof_mode        TEXT NOT NULL,
                 opt_verify_mode       TEXT NOT NULL,
                 opt_accelerator       TEXT NOT NULL,
                 result_path         TEXT,
                 error               TEXT,
                 error_code          TEXT
             );
             CREATE INDEX IF NOT EXISTS idx_jobs_status ON jobs(status);
             CREATE INDEX IF NOT EXISTS idx_jobs_finished_at ON jobs(finished_at);",
        )
        .map_err(|e| format!("failed to create schema: {e}"))?;

        Self::ensure_jobs_schema(&conn)?;

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

    fn ensure_jobs_schema(conn: &Connection) -> Result<(), String> {
        let mut columns = Self::jobs_columns(conn)?;
        for (column, migration_sql) in JOBS_SCHEMA_MIGRATIONS {
            if columns.contains(column) {
                continue;
            }

            tracing::warn!(column, "applying jobs table migration");
            conn.execute_batch(migration_sql)
                .map_err(|e| format!("failed to add jobs.{column}: {e}"))?;
            columns.insert(column.to_string());
        }
        Ok(())
    }

    fn jobs_columns(conn: &Connection) -> Result<HashSet<String>, String> {
        let mut stmt = conn
            .prepare("PRAGMA table_info(jobs)")
            .map_err(|e| format!("failed to read jobs table info: {e}"))?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|e| format!("failed to iterate jobs columns: {e}"))?;

        let mut columns = HashSet::new();
        for row in rows {
            columns.insert(row.map_err(|e| format!("failed to parse jobs column info: {e}"))?);
        }
        Ok(columns)
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
}
