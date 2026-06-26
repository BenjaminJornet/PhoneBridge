use crate::db;
use crate::path_utils::expand_home;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use tauri::{Emitter, Window};
use uuid::Uuid;
use walkdir::WalkDir;

use super::storage::{
    content_exists, content_exists_in_transaction, content_storage_path, copy_content, hash_file,
};
use super::{ConsolidationConfig, ConsolidationPlan, ConsolidationResult, HashedFile, LibraryError, SourceFile};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsolidationProgress {
    pub processed_files: u64,
    pub total_files: u64,
    pub current_path: String,
    pub copied_files: u64,
    pub duplicate_files: u64,
}

pub fn plan_consolidation(config: ConsolidationConfig) -> Result<ConsolidationPlan, LibraryError> {
    let connection = db::open_default_connection()?;
    plan_consolidation_with_connection(&connection, config)
}

pub(super) fn plan_consolidation_with_connection(
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
        already_on_computer: 0,
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
            if hash_indexed_outside(connection, &destination_path, &hash)? {
                plan.already_on_computer += 1;
            }
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

pub(super) fn execute_consolidation_with_connection(
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
        already_on_computer: 0,
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
            if hash_indexed_outside(&transaction, &destination_path, &hashed.hash)? {
                plan.already_on_computer += 1;
            }
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
          extension, size_bytes, modified_unix, content_hash, indexed_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, CURRENT_TIMESTAMP)
        ON CONFLICT(absolute_path) DO UPDATE SET
          root_path = excluded.root_path,
          relative_path = excluded.relative_path,
          category = excluded.category,
          source = excluded.source,
          extension = excluded.extension,
          size_bytes = excluded.size_bytes,
          modified_unix = excluded.modified_unix,
          content_hash = excluded.content_hash,
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
            file.hash,
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

/// True when this content hash is already indexed in a folder on the computer that
/// lives OUTSIDE the consolidated library (i.e. an external folder the user pointed
/// the gallery at, not a library object under `destination`). Used to flag, in the
/// import preview, files the user already has elsewhere. `&Transaction` coerces to
/// `&Connection` via `Deref`, so plan and execute share this one helper.
fn hash_indexed_outside(
    connection: &Connection,
    destination: &Path,
    hash: &str,
) -> Result<bool, LibraryError> {
    let mut statement =
        connection.prepare("SELECT absolute_path FROM files WHERE content_hash = ?1")?;
    let rows = statement.query_map(params![hash], |row| row.get::<_, String>(0))?;
    for row in rows {
        if !Path::new(&row?).starts_with(destination) {
            return Ok(true);
        }
    }
    Ok(false)
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
