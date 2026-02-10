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
        options: options_summary(&ProveOptions {
            max_frames: 18_000,
            segment_limit_po2: host::SEGMENT_LIMIT_PO2_DEFAULT,
            receipt_kind: host::ReceiptKind::Composite,
            proof_mode: host::ProofMode::Secure,
            verify_mode: host::VerifyMode::Verify,
        }),
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
            let kept_filename = format!("{}.json", job.job_id);
            conn.execute(
                "UPDATE jobs SET status = 'succeeded', finished_at = ?1,
                        result_path = ?2, error = NULL
                 WHERE job_id = ?3",
                params![now_unix_s() as i64, kept_filename, job.job_id.to_string()],
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
        let result_filename = format!("{}.json", job.job_id);
        conn.execute(
            "UPDATE jobs SET status = 'succeeded', finished_at = 1000,
                    result_path = ?1, error = NULL
             WHERE job_id = ?2",
            params![result_filename, job.job_id.to_string()],
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
        let result_filename = format!("{}.json", job.job_id);
        conn.execute(
            "UPDATE jobs SET status = 'succeeded', finished_at = ?1,
                    result_path = ?2, error = NULL
             WHERE job_id = ?3",
            params![now_unix_s() as i64, result_filename, job.job_id.to_string()],
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
