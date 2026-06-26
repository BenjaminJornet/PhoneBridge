use crate::db::DbError;
use serde::{Deserialize, Serialize};
use std::io;
use std::path::PathBuf;
use thiserror::Error;

mod consolidation;
mod coverage;
mod storage;

pub use consolidation::{execute_consolidation_with_progress, plan_consolidation};
pub use coverage::{list_backup_coverage, BackupCoverage};

#[derive(Debug, Error)]
pub enum LibraryError {
    #[error("database error: {0}")]
    Db(#[from] DbError),
    #[error("filesystem error: {0}")]
    Filesystem(#[from] io::Error),
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("walkdir error: {0}")]
    Walkdir(#[from] walkdir::Error),
    #[error("invalid source path: {0}")]
    InvalidSource(String),
    #[error("invalid destination path: {0}")]
    InvalidDestination(String),
    #[error("invalid relative path for {0}")]
    InvalidRelativePath(String),
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsolidationConfig {
    pub source_path: String,
    pub destination_path: String,
    pub adapter: String,
    pub label: String,
    pub device_id: Option<String>,
    pub device_label: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsolidationPlan {
    pub source_path: String,
    pub destination_path: String,
    pub total_files: u64,
    pub total_bytes: u64,
    pub new_files: u64,
    pub duplicate_files: u64,
    pub new_bytes: u64,
    pub duplicate_bytes: u64,
    /// Source files that are new to the library but already exist in a folder the
    /// user has indexed elsewhere on the computer. Informational only — these are
    /// still copied in, so the consolidated library stays self-contained.
    pub already_on_computer: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsolidationResult {
    pub backup_id: String,
    pub plan: ConsolidationPlan,
    pub copied_files: u64,
    pub duplicate_files: u64,
    pub copied_bytes: u64,
    pub occurrences_recorded: u64,
    pub errors: Vec<String>,
    pub run_id: String,
}

pub(super) struct SourceFile {
    absolute_path: PathBuf,
    relative_path: String,
    size_bytes: u64,
    modified_unix: Option<i64>,
    extension: Option<String>,
}

pub(super) struct HashedFile {
    source: SourceFile,
    hash: String,
    storage_path: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::consolidation::{
        execute_consolidation_with_connection, plan_consolidation_with_connection,
    };
    use super::coverage::list_backup_coverage_with_connection;
    use super::storage::hash_file;
    use super::*;
    use crate::db;
    use rusqlite::Connection;
    use sha2::{Digest, Sha256};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn plans_and_consolidates_content_with_provenance() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("source");
        let library = temp.path().join("library");
        fs::create_dir_all(source.join("DCIM")).unwrap();
        fs::write(source.join("DCIM/a.jpg"), [1, 2, 3]).unwrap();
        fs::write(source.join("DCIM/b.jpg"), [1, 2, 3]).unwrap();
        fs::write(source.join("DCIM/c.jpg"), [4, 5]).unwrap();

        let config = ConsolidationConfig {
            source_path: source.to_string_lossy().into_owned(),
            destination_path: library.to_string_lossy().into_owned(),
            adapter: "test".to_string(),
            label: "Test backup".to_string(),
            device_id: Some("device-1".to_string()),
            device_label: Some("Test Device".to_string()),
        };

        let mut connection = Connection::open_in_memory().unwrap();
        db::initialize(&connection).unwrap();

        let result =
            execute_consolidation_with_connection(&mut connection, config.clone(), None).unwrap();
        assert_eq!(result.plan.total_files, 3);
        assert_eq!(result.copied_files, 2);
        assert_eq!(result.duplicate_files, 1);
        assert_eq!(result.occurrences_recorded, 3);
        assert!(!result.run_id.is_empty());

        let plan = plan_consolidation_with_connection(&connection, config).unwrap();
        assert_eq!(plan.new_files, 0);
        assert_eq!(plan.duplicate_files, 3);

        let coverage = list_backup_coverage_with_connection(&connection).unwrap();
        let backup = coverage
            .iter()
            .find(|item| item.backup_id == result.backup_id)
            .unwrap();
        assert!(backup.safe_to_delete);
        assert_eq!(backup.total_files, 3);
        assert_eq!(backup.covered_files, 3);

        // Imported media must land in the gallery index, deduplicated and categorized.
        let metrics = db::category_metrics(&connection).unwrap();
        let photos = metrics
            .iter()
            .find(|item| item.category == "photo")
            .unwrap();
        // a.jpg and b.jpg share content (one object), c.jpg is a second object → 2 photos.
        assert_eq!(photos.count, 2);

        let gallery = db::list_indexed_files(&connection, Some("photo"), 10, 0).unwrap();
        assert_eq!(gallery.len(), 2);
        assert!(gallery
            .iter()
            .all(|file| file.absolute_path.contains("objects")));
    }

    #[test]
    fn flags_files_already_indexed_outside_the_library() {
        let temp = tempdir().unwrap();
        let external = temp.path().join("external");
        let source = temp.path().join("source");
        let library = temp.path().join("library");
        fs::create_dir_all(external.join("Photo")).unwrap();
        fs::create_dir_all(source.join("DCIM")).unwrap();
        // Same content lives in an external indexed folder and on the "phone".
        fs::write(external.join("Photo/keep.jpg"), [7, 7, 7, 7]).unwrap();
        fs::write(source.join("DCIM/from_phone.jpg"), [7, 7, 7, 7]).unwrap();

        let mut connection = Connection::open_in_memory().unwrap();
        db::initialize(&connection).unwrap();
        // Index the external folder so its hash lands in files.content_hash.
        db::index_multimedia(&mut connection, &external).unwrap();

        let config = ConsolidationConfig {
            source_path: source.to_string_lossy().into_owned(),
            destination_path: library.to_string_lossy().into_owned(),
            adapter: "test".to_string(),
            label: "Phone".to_string(),
            device_id: None,
            device_label: None,
        };
        let plan = plan_consolidation_with_connection(&connection, config).unwrap();
        // Not in the library yet, so it is still "new" and will be copied in…
        assert_eq!(plan.new_files, 1);
        // …but the preview flags that the user already has it in the external folder.
        assert_eq!(plan.already_on_computer, 1);
    }

    #[test]
    fn hashes_large_files_in_streaming_chunks() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("large.bin");
        let data = vec![0x5a; 1024 * 1024 + 123];
        std::fs::write(&path, &data).unwrap();

        let expected = format!("{:x}", Sha256::digest(&data));
        let actual = hash_file(&path).unwrap();

        assert_eq!(actual, expected);
    }
}
