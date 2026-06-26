use crate::db;
use rusqlite::Connection;
use serde::Serialize;

use super::LibraryError;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupCoverage {
    pub backup_id: String,
    pub label: String,
    pub source_path: String,
    pub total_files: u64,
    pub covered_files: u64,
    pub total_bytes: u64,
    pub covered_bytes: u64,
    pub coverage_percent: f64,
    pub reclaimable_bytes: u64,
    pub safe_to_delete: bool,
}

pub fn list_backup_coverage() -> Result<Vec<BackupCoverage>, LibraryError> {
    let connection = db::open_default_connection()?;
    list_backup_coverage_with_connection(&connection)
}

pub(super) fn list_backup_coverage_with_connection(
    connection: &Connection,
) -> Result<Vec<BackupCoverage>, LibraryError> {
    let mut statement = connection.prepare(
        "
        SELECT
          b.id,
          b.label,
          b.source_path,
          COUNT(o.id) AS total_files,
          COUNT(c.hash) AS covered_files,
          COALESCE(SUM(c.size_bytes), 0) AS total_bytes,
          COALESCE(SUM(CASE WHEN c.storage_path IS NOT NULL THEN c.size_bytes ELSE 0 END), 0) AS covered_bytes
        FROM backups b
        LEFT JOIN occurrences o ON o.backup_id = b.id
        LEFT JOIN contents c ON c.hash = o.content_hash
        GROUP BY b.id, b.label, b.source_path
        ORDER BY b.imported_at DESC
        ",
    )?;

    let rows = statement.query_map([], |row| {
        let total_files = row.get::<_, i64>(3)? as u64;
        let covered_files = row.get::<_, i64>(4)? as u64;
        let total_bytes = row.get::<_, i64>(5)? as u64;
        let covered_bytes = row.get::<_, i64>(6)? as u64;
        let coverage_percent = if total_files == 0 {
            0.0
        } else {
            (covered_files as f64 / total_files as f64) * 100.0
        };

        Ok(BackupCoverage {
            backup_id: row.get(0)?,
            label: row.get(1)?,
            source_path: row.get(2)?,
            total_files,
            covered_files,
            total_bytes,
            covered_bytes,
            coverage_percent,
            reclaimable_bytes: if total_files > 0 && total_files == covered_files {
                total_bytes
            } else {
                0
            },
            safe_to_delete: total_files > 0 && total_files == covered_files,
        })
    })?;

    let mut coverage = Vec::new();
    for row in rows {
        coverage.push(row?);
    }
    Ok(coverage)
}
