use std::{collections::HashSet, fs, path::PathBuf};

use super::*;

impl JobStore {
    fn result_filename_from_db_value(value: &str) -> Result<String, String> {
        let filename = value.trim();

        if filename.is_empty() {
            return Err("invalid result_path with empty filename".to_string());
        }
        if filename.contains('/') || filename.contains('\\') {
            return Err(format!(
                "invalid result_path (must be a filename, not a path): {value}"
            ));
        }
        if !filename.ends_with(".json") {
            return Err(format!(
                "invalid result_path extension (expected .json): {value}"
            ));
        }

        Ok(filename.to_string())
    }

    pub(super) fn result_path_from_db_value(&self, value: &str) -> Result<PathBuf, String> {
        let filename = Self::result_filename_from_db_value(value)?;
        Ok(self.results_dir.join(filename))
    }

    /// Best-effort cleanup of a stored result file path.
    pub(super) fn remove_result_file_best_effort(&self, stored_path: &str) {
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
    pub(super) fn cleanup_orphan_results(&self) -> Result<usize, String> {
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
}
