use std::{fs, str::FromStr};

use host::{accelerator, ReceiptKind};
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use super::*;

impl JobStore {
    #[cfg(test)]
    pub(crate) fn insert(&self, job: &ProofJob) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        Self::insert_row(&conn, job)
    }

    /// Insert one job row into SQLite.
    pub(super) fn insert_row(conn: &Connection, job: &ProofJob) -> Result<(), String> {
        conn.execute(
            "INSERT INTO jobs (
                job_id, status, created_at, started_at, finished_at,
                tape_size_bytes,
                opt_max_frames, opt_receipt_kind, opt_segment_limit_po2,
                opt_proof_mode, opt_verify_mode, opt_accelerator,
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
                job.options.proof_mode.as_str(),
                job.options.verify_mode.as_str(),
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
                        opt_proof_mode, opt_verify_mode, opt_accelerator,
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
                        opt_proof_mode: row.get(9)?,
                        opt_verify_mode: row.get(10)?,
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

    fn row_to_job(&self, r: RawJobRow) -> Result<ProofJob, String> {
        let job_id = Uuid::parse_str(&r.job_id).map_err(|e| format!("bad uuid in db: {e}"))?;
        let status = status_from_str(&r.status)?;
        let receipt_kind = ReceiptKind::from_str(&r.opt_receipt_kind)
            .map_err(|e| format!("bad receipt_kind in db: {e}"))?;
        let proof_mode = crate::ProofMode::from_str(&r.opt_proof_mode)
            .map_err(|e| format!("bad proof_mode in db: {e}"))?;
        let verify_mode = crate::VerifyMode::from_str(&r.opt_verify_mode)
            .map_err(|e| format!("bad verify_mode in db: {e}"))?;

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
                proof_mode,
                verify_mode,
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
    opt_proof_mode: String,
    opt_verify_mode: String,
    _opt_accelerator: String,
    result_path: Option<String>,
    error: Option<String>,
    error_code: Option<String>,
}

pub(super) fn status_to_str(status: JobStatus) -> &'static str {
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
