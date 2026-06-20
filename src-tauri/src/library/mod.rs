use crate::db::{self, DbError};
use crate::path_utils::expand_home;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::{Path, PathBuf};
use tauri::{Emitter, Window};
use thiserror::Error;
use uuid::Uuid;
use walkdir::WalkDir;

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

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsolidationProgress {
    pub processed_files: u64,
    pub total_files: u64,
    pub current_path: String,
    pub copied_files: u64,
    pub duplicate_files: u64,
}

struct SourceFile {
    absolute_path: PathBuf,
    relative_path: String,
    size_bytes: u64,
    modified_unix: Option<i64>,
    extension: Option<String>,
}

struct HashedFile {
    source: SourceFile,
    hash: String,
    storage_path: PathBuf,
}

pub fn plan_consolidation(config: ConsolidationConfig) -> Result<ConsolidationPlan, LibraryError> {
    let connection = db::open_default_connection()?;
    plan_consolidation_with_connection(&connection, config)
}

fn plan_consolidation_with_connection(
    connection: &Connection,
    config: ConsolidationConfig,
) -> Result<ConsolidationPlan, LibraryError> {
    let source_path = expand_home(&config.source_path);
    let destination_path = expand_home(&config.destination_path);
    validate_paths(&source_path, &destination_path, &config)?;

    let files = collect_source_files(&source_path)?;
    let mut plan = ConsolidationPlan {
        source_path: source_path.to_string_lossy().into_owned(),
        destination_path: destination_path.to_string_lossy().into_owned(),
        total_files: 0,
        total_bytes: 0,
        new_files: 0,
        duplicate_files: 0,
        new_bytes: 0,
        duplicate_bytes: 0,
    };

    for file in files {
        let hash = hash_file(&file.absolute_path)?;
        plan.total_files += 1;
        plan.total_bytes += file.size_bytes;
        if content_exists(connection, &hash)? {
            plan.duplicate_files += 1;
            plan.duplicate_bytes += file.size_bytes;
        } else {
            plan.new_files += 1;
            plan.new_bytes += file.size_bytes;
        }
    }

    Ok(plan)
}

pub fn execute_consolidation_with_progress(
    config: ConsolidationConfig,
    window: Window,
) -> Result<ConsolidationResult, LibraryError> {
    let mut connection = db::open_default_connection()?;
    execute_consolidation_with_connection(&mut connection, config, Some(window))
}

fn execute_consolidation_with_connection(
    connection: &mut Connection,
    config: ConsolidationConfig,
    window: Option<Window>,
) -> Result<ConsolidationResult, LibraryError> {
    let source_path = expand_home(&config.source_path);
    let destination_path = expand_home(&config.destination_path);
    validate_paths(&source_path, &destination_path, &config)?;
    fs::create_dir_all(&destination_path)?;
    let _lock = ImportLock::acquire(&destination_path)?;

    let files = collect_source_files(&source_path)?;
    let total_files = files.len() as u64;
    let backup_id = Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("{}:{}", config.adapter, source_path.display()).as_bytes(),
    )
    .to_string();
    let run_id = Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("run:{}:{}", backup_id, chrono_like_timestamp()).as_bytes(),
    )
    .to_string();

    let mut plan = ConsolidationPlan {
        source_path: source_path.to_string_lossy().into_owned(),
        destination_path: destination_path.to_string_lossy().into_owned(),
        total_files: 0,
        total_bytes: 0,
        new_files: 0,
        duplicate_files: 0,
        new_bytes: 0,
        duplicate_bytes: 0,
    };
    let mut copied_files = 0;
    let mut copied_bytes = 0;
    let mut duplicate_files = 0;
    let mut occurrences_recorded = 0;
    let mut errors = Vec::new();

    db::initialize(connection)?;
    let transaction = connection.transaction()?;
    transaction.execute(
        "INSERT INTO import_runs (id, source_path, destination_path, status) VALUES (?1, ?2, ?3, 'running')",
        params![run_id, source_path.to_string_lossy(), destination_path.to_string_lossy()],
    )?;
    if let Some(device_id) = config.device_id.as_deref() {
        transaction.execute(
            "
            INSERT INTO devices (id, label, manufacturer, model, android_version)
            VALUES (?1, ?2, NULL, NULL, NULL)
            ON CONFLICT(id) DO UPDATE SET label = excluded.label
            ",
            params![
                device_id,
                config.device_label.as_deref().unwrap_or("Unknown device")
            ],
        )?;
    }
    transaction.execute(
        "
        INSERT INTO backups (id, adapter, label, source_path, device_id, imported_at)
        VALUES (?1, ?2, ?3, ?4, ?5, CURRENT_TIMESTAMP)
        ON CONFLICT(id) DO UPDATE SET
          label = excluded.label,
          source_path = excluded.source_path,
          device_id = excluded.device_id,
          imported_at = CURRENT_TIMESTAMP
        ",
        params![
            backup_id,
            config.adapter,
            config.label,
            source_path.to_string_lossy(),
            config.device_id,
        ],
    )?;

    for file in files {
        let hash = hash_file(&file.absolute_path)?;
        let storage_path =
            content_storage_path(&destination_path, &hash, file.extension.as_deref());
        let hashed = HashedFile {
            source: file,
            hash,
            storage_path,
        };

        plan.total_files += 1;
        plan.total_bytes += hashed.source.size_bytes;

        if content_exists_in_transaction(&transaction, &hashed.hash)? {
            plan.duplicate_files += 1;
            plan.duplicate_bytes += hashed.source.size_bytes;
            duplicate_files += 1;
            insert_manifest_entry(&transaction, &run_id, &hashed, "duplicate")?;
        } else {
            plan.new_files += 1;
            plan.new_bytes += hashed.source.size_bytes;
            copy_content(&hashed)?;
            insert_content(&transaction, &hashed)?;
            copied_files += 1;
            copied_bytes += hashed.source.size_bytes;
            insert_manifest_entry(&transaction, &run_id, &hashed, "copied")?;
        }

        match insert_occurrence(&transaction, &backup_id, &hashed) {
            Ok(inserted) => occurrences_recorded += inserted,
            Err(cause) => errors.push(cause.to_string()),
        }

        // Index the deduplicated content object so it shows up in the gallery and
        // counts towards the dashboard category metrics. Without this the import
        // populated the content store but stayed invisible to the user.
        if let Err(cause) =
            index_imported_file(&transaction, &destination_path, &hashed, &config.label)
        {
            errors.push(cause.to_string());
        }

        if let Some(window) = &window {
            let _ = window.emit(
                "consolidation-progress",
                ConsolidationProgress {
                    processed_files: plan.total_files,
                    total_files,
                    current_path: hashed.source.relative_path.clone(),
                    copied_files,
                    duplicate_files,
                },
            );
        }
    }

    transaction.execute(
        "UPDATE import_runs SET status = 'completed', finished_at = CURRENT_TIMESTAMP WHERE id = ?1",
        params![run_id],
    )?;
    transaction.commit()?;

    Ok(ConsolidationResult {
        backup_id,
        plan,
        copied_files,
        duplicate_files,
        copied_bytes,
        occurrences_recorded,
        errors,
        run_id,
    })
}

/// Record the imported (deduplicated) content object in the `files` index that the
/// gallery and dashboard read. Keyed by the content-store path, so identical content
/// imported from several sources collapses to a single library entry.
fn index_imported_file(
    transaction: &rusqlite::Transaction<'_>,
    root_path: &Path,
    file: &HashedFile,
    source_label: &str,
) -> Result<(), LibraryError> {
    // For duplicates the file already lives at the path recorded in `contents`
    // (which may differ from `file.storage_path` when the destination changed).
    // Always resolve the canonical location so the Gallery can serve the image.
    let actual_storage_path: String = transaction
        .query_row(
            "SELECT storage_path FROM contents WHERE hash = ?1",
            params![file.hash],
            |row| row.get(0),
        )
        .unwrap_or_else(|_| file.storage_path.to_string_lossy().into_owned());

    let category =
        db::classify_media_category(&file.source.relative_path, file.source.extension.as_deref());
    transaction.execute(
        "
        INSERT INTO files (
          root_path, absolute_path, relative_path, category, source,
          extension, size_bytes, modified_unix, indexed_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, CURRENT_TIMESTAMP)
        ON CONFLICT(absolute_path) DO UPDATE SET
          root_path = excluded.root_path,
          relative_path = excluded.relative_path,
          category = excluded.category,
          source = excluded.source,
          extension = excluded.extension,
          size_bytes = excluded.size_bytes,
          modified_unix = excluded.modified_unix,
          indexed_at = CURRENT_TIMESTAMP
        ",
        params![
            root_path.to_string_lossy(),
            actual_storage_path,
            file.source.relative_path,
            category,
            source_label,
            file.source.extension,
            file.source.size_bytes as i64,
            file.source.modified_unix,
        ],
    )?;
    Ok(())
}

fn insert_manifest_entry(
    transaction: &rusqlite::Transaction<'_>,
    run_id: &str,
    file: &HashedFile,
    action: &str,
) -> Result<(), LibraryError> {
    transaction.execute(
        "
        INSERT INTO import_run_entries (run_id, original_path, content_hash, action, size_bytes)
        VALUES (?1, ?2, ?3, ?4, ?5)
        ",
        params![
            run_id,
            file.source.relative_path,
            file.hash,
            action,
            file.source.size_bytes as i64,
        ],
    )?;
    Ok(())
}

fn chrono_like_timestamp() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_nanos().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

pub fn list_backup_coverage() -> Result<Vec<BackupCoverage>, LibraryError> {
    let connection = db::open_default_connection()?;
    list_backup_coverage_with_connection(&connection)
}

fn list_backup_coverage_with_connection(
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

fn validate_paths(
    source_path: &Path,
    destination_path: &Path,
    config: &ConsolidationConfig,
) -> Result<(), LibraryError> {
    if !source_path.is_dir() {
        return Err(LibraryError::InvalidSource(config.source_path.clone()));
    }
    if destination_path.as_os_str().is_empty() {
        return Err(LibraryError::InvalidDestination(
            config.destination_path.clone(),
        ));
    }
    Ok(())
}

fn collect_source_files(source_path: &Path) -> Result<Vec<SourceFile>, LibraryError> {
    let mut files = Vec::new();
    for entry in WalkDir::new(source_path) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let metadata = entry.metadata()?;
        let relative_path = entry
            .path()
            .strip_prefix(source_path)
            .map_err(|_| LibraryError::InvalidRelativePath(entry.path().display().to_string()))?
            .to_string_lossy()
            .into_owned();
        let modified_unix = metadata
            .modified()
            .ok()
            .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|value| value.as_secs() as i64);
        let extension = entry
            .path()
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_ascii_lowercase());

        files.push(SourceFile {
            absolute_path: entry.path().to_path_buf(),
            relative_path,
            size_bytes: metadata.len(),
            modified_unix,
            extension,
        });
    }
    Ok(files)
}

fn hash_file(path: &Path) -> Result<String, LibraryError> {
    let mut reader = BufReader::new(File::open(path)?);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 64];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn content_exists(connection: &Connection, hash: &str) -> Result<bool, LibraryError> {
    Ok(connection
        .query_row(
            "SELECT 1 FROM contents WHERE hash = ?1",
            params![hash],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

fn content_exists_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    hash: &str,
) -> Result<bool, LibraryError> {
    Ok(transaction
        .query_row(
            "SELECT 1 FROM contents WHERE hash = ?1",
            params![hash],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

fn content_storage_path(destination_path: &Path, hash: &str, extension: Option<&str>) -> PathBuf {
    let shard = &hash[..2];
    let file_name = if let Some(extension) = extension.filter(|value| !value.is_empty()) {
        format!("{hash}.{extension}")
    } else {
        hash.to_string()
    };
    destination_path.join("objects").join(shard).join(file_name)
}

fn copy_content(file: &HashedFile) -> Result<(), LibraryError> {
    if let Some(parent) = file.storage_path.parent() {
        fs::create_dir_all(parent)?;
    }
    atomic_copy(&file.source.absolute_path, &file.storage_path)
}

fn atomic_copy(source: &Path, target: &Path) -> Result<(), LibraryError> {
    let tmp_target = temporary_copy_path(target);
    if tmp_target.exists() {
        fs::remove_file(&tmp_target)?;
    }
    fs::copy(source, &tmp_target)?;
    File::open(&tmp_target)?.sync_all()?;
    fs::rename(&tmp_target, target)?;
    Ok(())
}

fn temporary_copy_path(target: &Path) -> PathBuf {
    let file_name = target
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("phonebridge-object");
    target.with_file_name(format!(".{file_name}.phonebridge.tmp"))
}

struct ImportLock {
    path: PathBuf,
}

impl ImportLock {
    fn acquire(root: &Path) -> Result<Self, LibraryError> {
        let path = root.join(".phonebridge-import.lock");
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(_) => Ok(Self { path }),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                Err(LibraryError::InvalidDestination(format!(
                    "import already running at {}",
                    root.display()
                )))
            }
            Err(err) => Err(LibraryError::Filesystem(err)),
        }
    }
}

impl Drop for ImportLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn insert_content(
    transaction: &rusqlite::Transaction<'_>,
    file: &HashedFile,
) -> Result<(), LibraryError> {
    transaction.execute(
        "
        INSERT INTO contents (hash, size_bytes, extension, storage_path, first_seen_at)
        VALUES (?1, ?2, ?3, ?4, CURRENT_TIMESTAMP)
        ON CONFLICT(hash) DO NOTHING
        ",
        params![
            file.hash,
            file.source.size_bytes as i64,
            file.source.extension,
            file.storage_path.to_string_lossy()
        ],
    )?;
    Ok(())
}

fn insert_occurrence(
    transaction: &rusqlite::Transaction<'_>,
    backup_id: &str,
    file: &HashedFile,
) -> Result<u64, LibraryError> {
    let changed = transaction.execute(
        "
        INSERT INTO occurrences (content_hash, backup_id, original_path, original_mtime)
        VALUES (?1, ?2, ?3, ?4)
        ON CONFLICT(content_hash, backup_id, original_path) DO UPDATE SET
          original_mtime = excluded.original_mtime
        ",
        params![
            file.hash,
            backup_id,
            file.source.relative_path,
            file.source.modified_unix,
        ],
    )?;
    Ok(changed as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
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
