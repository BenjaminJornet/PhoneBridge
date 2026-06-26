use crate::adapters::CategoryMetric;
use crate::path_utils::expand_home;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use thiserror::Error;
use walkdir::WalkDir;

const APP_DIR_NAME: &str = ".phonebridge";
const DB_FILE_NAME: &str = "phonebridge.sqlite3";
const THUMBS_DIR_NAME: &str = "thumbs";
const MULTIMEDIA_CATEGORIES: [&str; 4] = ["Photo", "Video", "Music", "Documents"];
const CURRENT_SCHEMA_VERSION: i64 = 8;
// Max Hamming distance (out of 64 dHash bits) for two photos to count as look-alikes.
// Kept conservative: an 8x8 dHash is coarse, so a looser threshold lumps unrelated
// text-heavy screenshots together. Near-dup pairs (re-saved/resized) sit well under this.
const SIMILAR_HAMMING_THRESHOLD: u32 = 8;
// Extensions the `image` crate can decode directly (so we hash the original, not a thumbnail).
const IMAGE_DECODABLE_EXTENSIONS: &[&str] =
    &["jpg", "jpeg", "png", "gif", "webp", "bmp", "tif", "tiff"];
// Photo extensions that WebViews can't render natively → generate JPEG thumbnails via sips.
const NON_WEB_PHOTO_EXTENSIONS: &[&str] = &[
    "heic", "heif", "tif", "tiff", "dng", "raw", "cr2", "nef", "arw", "rw2", "orf",
];

#[derive(Debug, Error)]
pub enum DbError {
    #[error("filesystem error: {0}")]
    Filesystem(#[from] std::io::Error),
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("walkdir error: {0}")]
    Walkdir(#[from] walkdir::Error),
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexSummary {
    pub database_path: String,
    pub root_path: String,
    pub scanned_files: u64,
    pub indexed_files: u64,
    pub total_bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexedFile {
    pub id: i64,
    pub absolute_path: String,
    pub relative_path: String,
    pub category: String,
    pub source: String,
    pub extension: Option<String>,
    pub size_bytes: u64,
    pub modified_unix: Option<i64>,
    pub thumbnail_path: Option<String>,
}

/// A set of indexed files that share the exact same content (identical SHA-256).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateGroup {
    pub hash: String,
    pub size_bytes: u64,
    /// Bytes freed if every copy but one is removed: `size_bytes * (count - 1)`.
    pub reclaimable_bytes: u64,
    pub files: Vec<IndexedFile>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateScanResult {
    pub groups: Vec<DuplicateGroup>,
    pub total_groups: usize,
    pub reclaimable_bytes: u64,
    /// How many files were hashed (same-size candidates), for context in the UI.
    pub scanned_candidates: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashResult {
    pub trashed: usize,
    pub removed_from_index: usize,
    pub errors: Vec<String>,
}

pub fn open_default_connection() -> Result<Connection, DbError> {
    let path = default_database_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let connection = Connection::open(path)?;
    initialize(&connection)?;
    Ok(connection)
}

pub fn initialize(connection: &Connection) -> Result<(), DbError> {
    connection.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA foreign_keys = ON;
        ",
    )?;

    let version = schema_version(connection)?;
    if version < CURRENT_SCHEMA_VERSION {
        if version < 1 {
            migrate_to_v1(connection)?;
        }
        if version < 2 {
            migrate_to_v2(connection)?;
        }
        if version < 3 {
            migrate_to_v3(connection)?;
        }
        if version < 4 {
            migrate_to_v4(connection)?;
        }
        if version < 5 {
            migrate_to_v5(connection)?;
        }
        if version < 6 {
            migrate_to_v6(connection)?;
        }
        if version < 7 {
            migrate_to_v7(connection)?;
        }
        if version < 8 {
            migrate_to_v8(connection)?;
        }
    }

    Ok(())
}

fn schema_version(connection: &Connection) -> Result<i64, DbError> {
    Ok(connection.query_row("PRAGMA user_version", [], |row| row.get(0))?)
}

fn migrate_to_v1(connection: &Connection) -> Result<(), DbError> {
    connection.execute_batch(
        "

        CREATE TABLE IF NOT EXISTS scan_runs (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          root_path TEXT NOT NULL,
          scanned_files INTEGER NOT NULL,
          indexed_files INTEGER NOT NULL,
          total_bytes INTEGER NOT NULL,
          created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS files (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          root_path TEXT NOT NULL,
          absolute_path TEXT NOT NULL UNIQUE,
          relative_path TEXT NOT NULL,
          category TEXT NOT NULL,
          source TEXT NOT NULL,
          extension TEXT,
          size_bytes INTEGER NOT NULL,
          modified_unix INTEGER,
          indexed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE INDEX IF NOT EXISTS idx_files_category ON files(category);
        CREATE INDEX IF NOT EXISTS idx_files_source ON files(source);
        CREATE INDEX IF NOT EXISTS idx_files_size ON files(size_bytes);

        PRAGMA user_version = 1;
        ",
    )?;

    Ok(())
}

fn migrate_to_v2(connection: &Connection) -> Result<(), DbError> {
    connection.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS contents (
          hash TEXT PRIMARY KEY,
          size_bytes INTEGER NOT NULL,
          extension TEXT,
          storage_path TEXT NOT NULL,
          first_seen_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS backups (
          id TEXT PRIMARY KEY,
          adapter TEXT NOT NULL,
          label TEXT NOT NULL,
          source_path TEXT NOT NULL,
          imported_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS occurrences (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          content_hash TEXT NOT NULL REFERENCES contents(hash) ON DELETE CASCADE,
          backup_id TEXT NOT NULL REFERENCES backups(id) ON DELETE CASCADE,
          original_path TEXT NOT NULL,
          original_mtime INTEGER,
          UNIQUE(content_hash, backup_id, original_path)
        );

        CREATE INDEX IF NOT EXISTS idx_occurrences_backup ON occurrences(backup_id);
        CREATE INDEX IF NOT EXISTS idx_occurrences_content ON occurrences(content_hash);

        PRAGMA user_version = 2;
        ",
    )?;

    Ok(())
}

fn migrate_to_v3(connection: &Connection) -> Result<(), DbError> {
    connection.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS import_runs (
          id TEXT PRIMARY KEY,
          source_path TEXT NOT NULL,
          destination_path TEXT NOT NULL,
          status TEXT NOT NULL,
          started_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
          finished_at TEXT
        );

        CREATE TABLE IF NOT EXISTS import_run_entries (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          run_id TEXT NOT NULL REFERENCES import_runs(id) ON DELETE CASCADE,
          original_path TEXT NOT NULL,
          content_hash TEXT,
          action TEXT NOT NULL,
          size_bytes INTEGER NOT NULL,
          created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE INDEX IF NOT EXISTS idx_import_run_entries_run ON import_run_entries(run_id);

        PRAGMA user_version = 3;
        ",
    )?;

    Ok(())
}

fn migrate_to_v4(connection: &Connection) -> Result<(), DbError> {
    connection
        .execute_batch(
            "
        CREATE TABLE IF NOT EXISTS devices (
          id TEXT PRIMARY KEY,
          label TEXT NOT NULL,
          manufacturer TEXT,
          model TEXT,
          android_version TEXT,
          created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        ALTER TABLE backups ADD COLUMN device_id TEXT REFERENCES devices(id);

        PRAGMA user_version = 4;
        ",
        )
        .or_else(|err| {
            if err.to_string().contains("duplicate column name") {
                connection.execute_batch("PRAGMA user_version = 4;")?;
                Ok(())
            } else {
                Err(err)
            }
        })?;

    Ok(())
}

fn migrate_to_v5(connection: &Connection) -> Result<(), DbError> {
    // Earlier versions derived the media category from the first path segment only,
    // so real photos/videos imported from a phone or a SmartSwitch backup were filed
    // under garbage categories ("backup", "appdatas", ...) and never appeared in the
    // gallery. Recompute every existing row's category from its file extension.
    let mut rows: Vec<(i64, String, Option<String>)> = Vec::new();
    {
        let mut statement = connection.prepare("SELECT id, relative_path, extension FROM files")?;
        let mapped = statement.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?;
        for row in mapped {
            rows.push(row?);
        }
    }
    {
        let mut update = connection.prepare("UPDATE files SET category = ?1 WHERE id = ?2")?;
        for (id, relative_path, extension) in rows {
            let category = classify_media_category(&relative_path, extension.as_deref());
            update.execute(params![category, id])?;
        }
    }
    connection.execute_batch("PRAGMA user_version = 5;")?;

    Ok(())
}

fn migrate_to_v6(connection: &Connection) -> Result<(), DbError> {
    connection
        .execute_batch(
            "
        ALTER TABLE files ADD COLUMN thumbnail_path TEXT;
        PRAGMA user_version = 6;
        ",
        )
        .or_else(|err| {
            if err.to_string().contains("duplicate column name") {
                connection.execute_batch("PRAGMA user_version = 6;")?;
                Ok(())
            } else {
                Err(err)
            }
        })?;
    Ok(())
}

fn migrate_to_v7(connection: &Connection) -> Result<(), DbError> {
    // Store the SHA-256 of each indexed file so the duplicate finder can reuse a
    // previously computed hash instead of re-reading the file on every scan.
    connection
        .execute_batch(
            "
        ALTER TABLE files ADD COLUMN content_hash TEXT;
        CREATE INDEX IF NOT EXISTS idx_files_content_hash ON files(content_hash);
        PRAGMA user_version = 7;
        ",
        )
        .or_else(|err| {
            if err.to_string().contains("duplicate column name") {
                connection.execute_batch(
                    "
                    CREATE INDEX IF NOT EXISTS idx_files_content_hash ON files(content_hash);
                    PRAGMA user_version = 7;
                    ",
                )?;
                Ok(())
            } else {
                Err(err)
            }
        })?;
    Ok(())
}

fn migrate_to_v8(connection: &Connection) -> Result<(), DbError> {
    // Store a 64-bit perceptual hash (dHash) per photo so the near-duplicate finder
    // can reuse it instead of decoding every image on each scan.
    connection
        .execute_batch(
            "
        ALTER TABLE files ADD COLUMN perceptual_hash INTEGER;
        CREATE INDEX IF NOT EXISTS idx_files_perceptual_hash ON files(perceptual_hash);
        PRAGMA user_version = 8;
        ",
        )
        .or_else(|err| {
            if err.to_string().contains("duplicate column name") {
                connection.execute_batch(
                    "
                    CREATE INDEX IF NOT EXISTS idx_files_perceptual_hash ON files(perceptual_hash);
                    PRAGMA user_version = 8;
                    ",
                )?;
                Ok(())
            } else {
                Err(err)
            }
        })?;
    Ok(())
}

fn default_thumbnails_dir() -> Result<PathBuf, DbError> {
    Ok(env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(APP_DIR_NAME)
        .join(THUMBS_DIR_NAME))
}

fn needs_thumbnail(extension: &str) -> bool {
    let lower = extension.to_ascii_lowercase();
    NON_WEB_PHOTO_EXTENSIONS.contains(&lower.as_str())
}

/// Generate a JPEG thumbnail for non-web-displayable photos using the macOS `sips` utility.
/// Returns the thumbnail path if generation succeeds; `None` if sips is unavailable or fails.
/// Idempotent: skips sips if the output file already exists.
fn generate_thumbnail(source: &Path, thumbs_dir: &Path) -> Option<PathBuf> {
    let source_str = source.to_string_lossy();
    let mut hasher = Sha256::new();
    hasher.update(source_str.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    let output = thumbs_dir.join(format!("{}.jpg", &hash[..32]));

    if output.exists() {
        return Some(output);
    }
    if fs::create_dir_all(thumbs_dir).is_err() {
        return None;
    }
    let result = std::process::Command::new("sips")
        .args(["-s", "format", "jpeg", "-s", "formatOptions", "75", "-Z", "512"])
        .arg(source.as_os_str())
        .arg("--out")
        .arg(output.as_os_str())
        .output();
    match result {
        Ok(cmd) if cmd.status.success() && output.exists() => Some(output),
        _ => None,
    }
}

pub fn index_folder(source_path: String) -> Result<IndexSummary, DbError> {
    let root = expand_home(&source_path);
    let mut connection = open_default_connection()?;
    index_multimedia(&mut connection, &root)
}

pub fn get_indexed_category_metrics() -> Result<Vec<CategoryMetric>, DbError> {
    let connection = open_default_connection()?;
    category_metrics(&connection)
}

pub fn list_default_indexed_files(
    category: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<Vec<IndexedFile>, DbError> {
    let connection = open_default_connection()?;
    list_indexed_files(
        &connection,
        category.as_deref(),
        limit.unwrap_or(120),
        offset.unwrap_or(0),
    )
}

pub fn index_multimedia(connection: &mut Connection, root: &Path) -> Result<IndexSummary, DbError> {
    initialize(connection)?;
    let root_path = root.to_string_lossy().into_owned();
    let database_path = default_database_path()?.to_string_lossy().into_owned();

    if !root.exists() {
        return Ok(IndexSummary {
            database_path,
            root_path,
            scanned_files: 0,
            indexed_files: 0,
            total_bytes: 0,
        });
    }

    let thumbs_dir = default_thumbnails_dir().ok();

    // Reuse a previously computed content hash when a file is unchanged (same size
    // and mtime), so re-indexing a folder does not re-read every byte. New or
    // modified files are hashed once here, up front, so the duplicate finder and
    // the import dedup-vs-indexed check are instant afterwards.
    let mut existing_hashes: HashMap<String, (i64, Option<i64>, Option<String>)> = HashMap::new();
    {
        let mut statement = connection
            .prepare("SELECT absolute_path, size_bytes, modified_unix, content_hash FROM files")?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<i64>>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })?;
        for row in rows {
            let (path, size, mtime, hash) = row?;
            existing_hashes.insert(path, (size, mtime, hash));
        }
    }

    let transaction = connection.transaction()?;
    let mut scanned_files = 0;
    let mut indexed_files = 0;
    let mut total_bytes = 0;

    {
        let mut statement = transaction.prepare(
            "
            INSERT INTO files (
              root_path,
              absolute_path,
              relative_path,
              category,
              source,
              extension,
              size_bytes,
              modified_unix,
              thumbnail_path,
              content_hash,
              indexed_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, CURRENT_TIMESTAMP)
            ON CONFLICT(absolute_path) DO UPDATE SET
              root_path = excluded.root_path,
              relative_path = excluded.relative_path,
              category = excluded.category,
              source = excluded.source,
              extension = excluded.extension,
              size_bytes = excluded.size_bytes,
              modified_unix = excluded.modified_unix,
              thumbnail_path = excluded.thumbnail_path,
              content_hash = excluded.content_hash,
              indexed_at = CURRENT_TIMESTAMP
            ",
        )?;

        for entry in WalkDir::new(root) {
            let entry = entry?;
            if !entry.file_type().is_file() {
                continue;
            }

            let metadata = entry.metadata()?;
            let absolute_path = entry.path().to_string_lossy().into_owned();
            let relative_path = relative_path(root, entry.path());
            let extension = entry
                .path()
                .extension()
                .and_then(|value| value.to_str())
                .map(|value| value.to_ascii_lowercase());
            let category = classify_media_category(&relative_path, extension.as_deref());
            let source = source_from_relative_path(&relative_path);
            let modified_unix = metadata
                .modified()
                .ok()
                .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|value| value.as_secs() as i64);
            let size_bytes = metadata.len();

            let thumbnail_path = thumbs_dir.as_ref()
                .filter(|_| {
                    category == "photo"
                        && extension.as_deref().map(needs_thumbnail).unwrap_or(false)
                })
                .and_then(|dir| generate_thumbnail(entry.path(), dir))
                .map(|p| p.to_string_lossy().into_owned());

            // Reuse the stored hash if the file is byte-for-byte unchanged; otherwise
            // hash it now. A read failure (permissions, vanished file) leaves the hash
            // NULL and the file is simply hashed lazily by the duplicate scanner later.
            let content_hash = match existing_hashes.get(&absolute_path) {
                Some((prev_size, prev_mtime, Some(prev_hash)))
                    if *prev_size == size_bytes as i64 && *prev_mtime == modified_unix =>
                {
                    Some(prev_hash.clone())
                }
                _ => hash_file_contents(entry.path()).ok(),
            };

            scanned_files += 1;
            total_bytes += size_bytes;
            indexed_files += statement.execute(params![
                root_path,
                absolute_path,
                relative_path,
                category,
                source,
                extension,
                size_bytes as i64,
                modified_unix,
                thumbnail_path,
                content_hash,
            ])? as u64;
        }
    }

    transaction.execute(
        "
        INSERT INTO scan_runs (root_path, scanned_files, indexed_files, total_bytes)
        VALUES (?1, ?2, ?3, ?4)
        ",
        params![
            root_path,
            scanned_files as i64,
            indexed_files as i64,
            total_bytes as i64
        ],
    )?;
    transaction.commit()?;

    Ok(IndexSummary {
        database_path,
        root_path,
        scanned_files,
        indexed_files,
        total_bytes,
    })
}

pub fn category_metrics(connection: &Connection) -> Result<Vec<CategoryMetric>, DbError> {
    initialize(connection)?;
    let mut metrics = Vec::new();

    for category in MULTIMEDIA_CATEGORIES {
        let category_key = category.to_lowercase();
        let metric = connection
            .query_row(
                "
                SELECT COUNT(*), COALESCE(SUM(size_bytes), 0)
                FROM files
                WHERE category = ?1
                ",
                params![category_key],
                |row| {
                    Ok(CategoryMetric {
                        category: category.to_lowercase(),
                        count: row.get::<_, i64>(0)? as u64,
                        bytes: row.get::<_, i64>(1)? as u64,
                    })
                },
            )
            .optional()?;

        metrics.push(metric.unwrap_or(CategoryMetric {
            category: category_key,
            count: 0,
            bytes: 0,
        }));
    }

    Ok(metrics)
}

pub fn list_indexed_files(
    connection: &Connection,
    category: Option<&str>,
    limit: u32,
    offset: u32,
) -> Result<Vec<IndexedFile>, DbError> {
    initialize(connection)?;
    let safe_limit = limit.clamp(1, 500) as i64;
    let safe_offset = offset as i64;

    let sql = if category.is_some() {
        "
        SELECT id, absolute_path, relative_path, category, source, extension, size_bytes, modified_unix, thumbnail_path
        FROM files
        WHERE category = ?1
        ORDER BY COALESCE(modified_unix, 0) DESC, size_bytes DESC
        LIMIT ?2 OFFSET ?3
        "
    } else {
        "
        SELECT id, absolute_path, relative_path, category, source, extension, size_bytes, modified_unix, thumbnail_path
        FROM files
        ORDER BY COALESCE(modified_unix, 0) DESC, size_bytes DESC
        LIMIT ?1 OFFSET ?2
        "
    };

    let mut statement = connection.prepare(sql)?;
    let rows = if let Some(category) = category {
        statement.query_map(
            params![category, safe_limit, safe_offset],
            indexed_file_from_row,
        )?
    } else {
        statement.query_map(params![safe_limit, safe_offset], indexed_file_from_row)?
    };

    let mut files = Vec::new();
    for row in rows {
        files.push(row?);
    }

    Ok(files)
}

/// Stream a file through SHA-256 in 64 KiB chunks. Kept local to the `db` module
/// (rather than reusing `library::hash_file`) so the lower-level `db` layer does
/// not depend on the higher-level `library` consolidation module.
fn hash_file_contents(path: &Path) -> Result<String, DbError> {
    let mut reader = BufReader::new(fs::File::open(path)?);
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

pub fn find_default_duplicate_files<F: FnMut(usize, usize)>(
    category: Option<String>,
    mut progress: F,
) -> Result<DuplicateScanResult, DbError> {
    let connection = open_default_connection()?;
    find_duplicate_files(&connection, category.as_deref(), &mut progress)
}

/// Find groups of indexed files that share identical content.
///
/// Two-pass to avoid hashing everything: pass 1 buckets files by `size_bytes`
/// and discards unique sizes (a content match implies an exact size match);
/// pass 2 hashes only the same-size candidates, persisting each hash back into
/// `files.content_hash`. `progress(done, total)` is called once per hashed file.
pub fn find_duplicate_files(
    connection: &Connection,
    category: Option<&str>,
    progress: &mut dyn FnMut(usize, usize),
) -> Result<DuplicateScanResult, DbError> {
    initialize(connection)?;

    let sql = if category.is_some() {
        "SELECT id, absolute_path, relative_path, category, source, extension, size_bytes, modified_unix, thumbnail_path, content_hash
         FROM files WHERE category = ?1"
    } else {
        "SELECT id, absolute_path, relative_path, category, source, extension, size_bytes, modified_unix, thumbnail_path, content_hash
         FROM files"
    };

    // Carry the stored content_hash (column 9) alongside each file so an already
    // hashed file can be grouped without re-reading it from disk.
    let map_row = |row: &rusqlite::Row<'_>| -> rusqlite::Result<(IndexedFile, Option<String>)> {
        Ok((indexed_file_from_row(row)?, row.get(9)?))
    };
    let mut statement = connection.prepare(sql)?;
    let rows = if let Some(category) = category {
        statement.query_map(params![category], map_row)?
    } else {
        statement.query_map([], map_row)?
    };
    let mut all: Vec<(IndexedFile, Option<String>)> = Vec::new();
    for row in rows {
        all.push(row?);
    }

    // Pass 1: keep only files whose size collides with at least one other file.
    let mut by_size: HashMap<u64, Vec<(IndexedFile, Option<String>)>> = HashMap::new();
    for entry in all {
        by_size.entry(entry.0.size_bytes).or_default().push(entry);
    }
    let candidates: Vec<(IndexedFile, Option<String>)> = by_size
        .into_values()
        .filter(|bucket| bucket.len() >= 2)
        .flatten()
        .collect();
    let total = candidates.len();

    // Pass 2: bucket by content hash, reusing the stored hash when present and only
    // reading from disk for files indexed before hashing existed.
    let mut by_hash: HashMap<String, Vec<IndexedFile>> = HashMap::new();
    let mut done = 0;
    for (file, stored_hash) in candidates {
        done += 1;
        progress(done, total);
        let hash = match stored_hash {
            Some(hash) => hash,
            // A file may have been moved or deleted on disk since indexing — skip it.
            None => match hash_file_contents(Path::new(&file.absolute_path)) {
                Ok(hash) => {
                    let _ = connection.execute(
                        "UPDATE files SET content_hash = ?1 WHERE id = ?2",
                        params![hash, file.id],
                    );
                    hash
                }
                Err(_) => continue,
            },
        };
        by_hash.entry(hash).or_default().push(file);
    }

    let mut groups: Vec<DuplicateGroup> = by_hash
        .into_iter()
        .filter(|(_, files)| files.len() >= 2)
        .map(|(hash, mut files)| {
            // Oldest first, stable by id — the UI keeps files[0] by default.
            files.sort_by_key(|file| (file.modified_unix.unwrap_or(i64::MAX), file.id));
            let size_bytes = files[0].size_bytes;
            let reclaimable_bytes = size_bytes * (files.len() as u64 - 1);
            DuplicateGroup {
                hash,
                size_bytes,
                reclaimable_bytes,
                files,
            }
        })
        .collect();
    groups.sort_by(|a, b| b.reclaimable_bytes.cmp(&a.reclaimable_bytes));

    let total_groups = groups.len();
    let reclaimable_bytes = groups.iter().map(|group| group.reclaimable_bytes).sum();

    Ok(DuplicateScanResult {
        groups,
        total_groups,
        reclaimable_bytes,
        scanned_candidates: total,
    })
}

/// Compute a 64-bit difference hash (dHash) for an image. Returns `None` if the file
/// can't be decoded by the `image` crate (HEIC originals, for instance — callers pass
/// the JPEG thumbnail for those).
fn perceptual_hash_of(path: &Path) -> Option<u64> {
    let image = image::open(path).ok()?;
    // 9x8 grayscale → compare each pixel to its right neighbour → 8x8 = 64 bits.
    let small = image
        .resize_exact(9, 8, image::imageops::FilterType::Triangle)
        .to_luma8();
    let mut hash: u64 = 0;
    let mut bit = 0;
    for y in 0..8u32 {
        for x in 0..8u32 {
            if small.get_pixel(x, y)[0] < small.get_pixel(x + 1, y)[0] {
                hash |= 1u64 << bit;
            }
            bit += 1;
        }
    }
    Some(hash)
}

/// Pick a path the `image` crate can decode: the JPEG thumbnail when present (HEIC,
/// RAW, …), otherwise the original if its extension is directly decodable.
fn decodable_image_path(file: &IndexedFile) -> Option<PathBuf> {
    if let Some(thumb) = &file.thumbnail_path {
        return Some(PathBuf::from(thumb));
    }
    let ext = file.extension.as_deref()?.to_ascii_lowercase();
    if IMAGE_DECODABLE_EXTENSIONS.contains(&ext.as_str()) {
        Some(PathBuf::from(&file.absolute_path))
    } else {
        None
    }
}

/// Union-find root with path halving.
fn uf_find(parent: &mut [usize], mut x: usize) -> usize {
    while parent[x] != x {
        parent[x] = parent[parent[x]];
        x = parent[x];
    }
    x
}

pub fn find_default_similar_photos<F: FnMut(usize, usize)>(
    mut progress: F,
) -> Result<DuplicateScanResult, DbError> {
    let connection = open_default_connection()?;
    find_similar_photos(&connection, &mut progress)
}

/// Group photos that look alike (not just byte-identical) by perceptual hash.
///
/// Decodes each photo once (thumbnail for HEIC/RAW, original otherwise), caches the
/// dHash in `files.perceptual_hash` so later scans are instant, then clusters by
/// Hamming distance with a union-find. Groups come back largest-file-first so the UI
/// keeps the highest-quality copy by default. Reuses `DuplicateScanResult` for shape.
pub fn find_similar_photos(
    connection: &Connection,
    progress: &mut dyn FnMut(usize, usize),
) -> Result<DuplicateScanResult, DbError> {
    initialize(connection)?;

    // Perceptual hash (column 9) carried alongside each photo.
    let map_row = |row: &rusqlite::Row<'_>| -> rusqlite::Result<(IndexedFile, Option<i64>)> {
        Ok((indexed_file_from_row(row)?, row.get(9)?))
    };
    let mut statement = connection.prepare(
        "SELECT id, absolute_path, relative_path, category, source, extension, size_bytes, modified_unix, thumbnail_path, perceptual_hash
         FROM files WHERE category = 'photo'",
    )?;
    let rows = statement.query_map([], map_row)?;
    let mut photos: Vec<(IndexedFile, Option<i64>)> = Vec::new();
    for row in rows {
        photos.push(row?);
    }

    // Compute (or reuse) the dHash for each photo, skipping any with no decodable source.
    let total = photos.len();
    let mut hashed: Vec<(IndexedFile, u64)> = Vec::new();
    for (done, (file, stored)) in photos.into_iter().enumerate() {
        progress(done + 1, total);
        let hash = match stored {
            Some(value) => value as u64,
            None => {
                let Some(path) = decodable_image_path(&file) else {
                    continue;
                };
                let Some(hash) = perceptual_hash_of(&path) else {
                    continue;
                };
                let _ = connection.execute(
                    "UPDATE files SET perceptual_hash = ?1 WHERE id = ?2",
                    params![hash as i64, file.id],
                );
                hash
            }
        };
        hashed.push((file, hash));
    }

    // Cluster by Hamming distance with a union-find over the hashed photos.
    let n = hashed.len();
    let mut parent: Vec<usize> = (0..n).collect();
    for i in 0..n {
        for j in (i + 1)..n {
            if (hashed[i].1 ^ hashed[j].1).count_ones() <= SIMILAR_HAMMING_THRESHOLD {
                let (ri, rj) = (uf_find(&mut parent, i), uf_find(&mut parent, j));
                if ri != rj {
                    parent[ri] = rj;
                }
            }
        }
    }

    let mut clusters: HashMap<usize, Vec<IndexedFile>> = HashMap::new();
    for (index, (file, _)) in hashed.into_iter().enumerate() {
        let root = uf_find(&mut parent, index);
        clusters.entry(root).or_default().push(file);
    }

    let mut groups: Vec<DuplicateGroup> = clusters
        .into_values()
        .filter(|files| files.len() >= 2)
        .map(|mut files| {
            // Largest first → the UI keeps the highest-quality copy (files[0]).
            files.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes).then(a.id.cmp(&b.id)));
            let largest = files[0].size_bytes;
            let total_bytes: u64 = files.iter().map(|file| file.size_bytes).sum();
            DuplicateGroup {
                hash: format!("sim-{}", files[0].id),
                size_bytes: largest,
                reclaimable_bytes: total_bytes - largest,
                files,
            }
        })
        .collect();
    groups.sort_by(|a, b| b.reclaimable_bytes.cmp(&a.reclaimable_bytes));

    let total_groups = groups.len();
    let reclaimable_bytes = groups.iter().map(|group| group.reclaimable_bytes).sum();
    Ok(DuplicateScanResult {
        groups,
        total_groups,
        reclaimable_bytes,
        scanned_candidates: total,
    })
}

pub fn move_files_to_trash(paths: Vec<String>) -> Result<TrashResult, DbError> {
    let connection = open_default_connection()?;
    move_files_to_trash_with_connection(&connection, paths)
}

/// Move the given files to the OS trash (recoverable) and drop their rows from
/// the index. Refuses any path inside PhoneBridge's own app directory so the
/// consolidated library can never be deleted by the tool (safe-purge guardrail).
pub fn move_files_to_trash_with_connection(
    connection: &Connection,
    paths: Vec<String>,
) -> Result<TrashResult, DbError> {
    initialize(connection)?;
    let app_root = default_app_dir()?;
    let mut errors = Vec::new();
    let mut safe_paths = Vec::new();

    for raw in paths {
        let path = PathBuf::from(&raw);
        if path.starts_with(&app_root) {
            errors.push(format!(
                "Refused to trash a file inside PhoneBridge's own storage: {raw}"
            ));
            continue;
        }
        if !path.exists() {
            errors.push(format!("File no longer exists: {raw}"));
            continue;
        }
        safe_paths.push(raw);
    }

    let mut trashed_paths = Vec::new();
    for path in &safe_paths {
        match trash::delete(path) {
            Ok(()) => trashed_paths.push(path.clone()),
            Err(err) => errors.push(format!("{path}: {err}")),
        }
    }

    let mut removed_from_index = 0;
    for path in &trashed_paths {
        removed_from_index += connection.execute(
            "DELETE FROM files WHERE absolute_path = ?1",
            params![path],
        )? as usize;
    }

    Ok(TrashResult {
        trashed: trashed_paths.len(),
        removed_from_index,
        errors,
    })
}

fn default_app_dir() -> Result<PathBuf, DbError> {
    Ok(env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(APP_DIR_NAME))
}

fn indexed_file_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<IndexedFile> {
    Ok(IndexedFile {
        id: row.get(0)?,
        absolute_path: row.get(1)?,
        relative_path: row.get(2)?,
        category: row.get(3)?,
        source: row.get(4)?,
        extension: row.get(5)?,
        size_bytes: row.get::<_, i64>(6)? as u64,
        modified_unix: row.get(7)?,
        thumbnail_path: row.get(8)?,
    })
}

fn default_database_path() -> Result<PathBuf, DbError> {
    Ok(env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(APP_DIR_NAME)
        .join(DB_FILE_NAME))
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

fn category_from_relative_path(path: &str) -> String {
    path.split(std::path::MAIN_SEPARATOR)
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or("Other")
        .to_ascii_lowercase()
}

/// Map a file extension to one of the gallery's media buckets.
fn media_category_for_extension(extension: &str) -> Option<&'static str> {
    match extension {
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" | "heic" | "heif" | "tif" | "tiff"
        | "dng" | "raw" | "cr2" | "nef" | "arw" | "rw2" | "orf" | "svg" => Some("photo"),
        "mp4" | "mov" | "m4v" | "mkv" | "avi" | "webm" | "3gp" | "3gpp" | "mpg" | "mpeg"
        | "wmv" | "flv" | "ts" | "m2ts" => Some("video"),
        "mp3" | "m4a" | "aac" | "flac" | "wav" | "ogg" | "oga" | "opus" | "wma" | "amr"
        | "aiff" | "aif" | "mid" | "midi" => Some("music"),
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "rtf" | "odt"
        | "ods" | "odp" | "csv" | "md" | "epub" | "pages" | "numbers" | "key" => Some("documents"),
        _ => None,
    }
}

/// Classify an indexed file into a gallery bucket (`photo`/`video`/`music`/`documents`),
/// extension first, then falling back to a leading `Photo/`, `Video/`, ... folder
/// (the layout produced by the SmartSwitch category sync). Anything else is `other`.
pub fn classify_media_category(relative_path: &str, extension: Option<&str>) -> String {
    if let Some(extension) = extension {
        if let Some(category) = media_category_for_extension(&extension.to_ascii_lowercase()) {
            return category.to_string();
        }
    }

    match category_from_relative_path(relative_path).as_str() {
        leading @ ("photo" | "video" | "music" | "documents") => leading.to_string(),
        _ => "other".to_string(),
    }
}

fn source_from_relative_path(path: &str) -> String {
    let mut parts = path.split(std::path::MAIN_SEPARATOR);
    let _category = parts.next();
    let Some(source) = parts.next() else {
        return "local".to_string();
    };

    if parts.next().is_none() {
        return "local".to_string();
    }

    source.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn indexes_multimedia_files_and_returns_metrics() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("Multimedia");
        fs::create_dir_all(root.join("Photo/DCIM")).unwrap();
        fs::create_dir_all(root.join("Video/Camera")).unwrap();
        fs::write(root.join("Photo/DCIM/image.jpg"), [1, 2, 3]).unwrap();
        fs::write(root.join("Video/Camera/movie.mp4"), [1, 2, 3, 4, 5]).unwrap();

        let mut connection = Connection::open_in_memory().unwrap();
        let summary = index_multimedia(&mut connection, &root).unwrap();
        assert_eq!(summary.scanned_files, 2);
        assert_eq!(summary.total_bytes, 8);

        let metrics = category_metrics(&connection).unwrap();
        let photo = metrics
            .iter()
            .find(|item| item.category == "photo")
            .unwrap();
        let video = metrics
            .iter()
            .find(|item| item.category == "video")
            .unwrap();
        assert_eq!(photo.count, 1);
        assert_eq!(photo.bytes, 3);
        assert_eq!(video.count, 1);
        assert_eq!(video.bytes, 5);

        let files = list_indexed_files(&connection, Some("photo"), 10, 0).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].relative_path, "Photo/DCIM/image.jpg");
        assert_eq!(files[0].source, "DCIM");
    }

    #[test]
    fn paginates_indexed_files_with_offset() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("Multimedia");
        fs::create_dir_all(root.join("Photo/DCIM")).unwrap();
        fs::write(root.join("Photo/DCIM/a.jpg"), [1]).unwrap();
        fs::write(root.join("Photo/DCIM/b.jpg"), [2]).unwrap();
        fs::write(root.join("Photo/DCIM/c.jpg"), [3]).unwrap();

        let mut connection = Connection::open_in_memory().unwrap();
        index_multimedia(&mut connection, &root).unwrap();

        let first_page = list_indexed_files(&connection, Some("photo"), 2, 0).unwrap();
        let second_page = list_indexed_files(&connection, Some("photo"), 2, 2).unwrap();

        assert_eq!(first_page.len(), 2);
        assert_eq!(second_page.len(), 1);
        assert_ne!(first_page[0].id, second_page[0].id);
    }

    #[test]
    fn classifies_media_by_extension_then_by_leading_folder() {
        // Extension wins, regardless of where the file sits.
        assert_eq!(
            classify_media_category("Camera/IMG_1.jpg", Some("jpg")),
            "photo"
        );
        assert_eq!(
            classify_media_category("backup/clip.mp4", Some("mp4")),
            "video"
        );
        assert_eq!(
            classify_media_category("x/y/song.flac", Some("flac")),
            "music"
        );
        assert_eq!(
            classify_media_category("export/report.pdf", Some("pdf")),
            "documents"
        );
        // Unknown extension falls back to a leading category folder (SmartSwitch layout).
        assert_eq!(
            classify_media_category("Photo/DCIM/file.bin", Some("bin")),
            "photo"
        );
        // Otherwise it is filed under `other`, never a garbage bucket.
        assert_eq!(
            classify_media_category("backup/app.apk", Some("apk")),
            "other"
        );
        assert_eq!(classify_media_category("backup/data.plist", None), "other");
    }

    #[test]
    fn uses_local_source_for_files_at_category_root() {
        assert_eq!(source_from_relative_path("Photo/image.jpg"), "local");
        assert_eq!(source_from_relative_path("Photo/DCIM/image.jpg"), "DCIM");
    }

    #[test]
    fn finds_exact_duplicate_groups_by_content() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("Multimedia");
        fs::create_dir_all(root.join("Photo/DCIM")).unwrap();
        // Two byte-identical files under different names + one unique file.
        fs::write(root.join("Photo/DCIM/original.jpg"), [9, 9, 9, 9]).unwrap();
        fs::write(root.join("Photo/DCIM/DUPLICATE_original.jpg"), [9, 9, 9, 9]).unwrap();
        fs::write(root.join("Photo/DCIM/unique.jpg"), [1, 2, 3, 4, 5]).unwrap();

        let mut connection = Connection::open_in_memory().unwrap();
        index_multimedia(&mut connection, &root).unwrap();

        let mut calls = 0;
        let result =
            find_duplicate_files(&connection, None, &mut |_done, _total| calls += 1).unwrap();

        // Only the two same-size files are hashed (the unique size is skipped).
        assert_eq!(result.scanned_candidates, 2);
        assert_eq!(calls, 2);
        assert_eq!(result.total_groups, 1);
        let group = &result.groups[0];
        assert_eq!(group.files.len(), 2);
        assert_eq!(group.size_bytes, 4);
        assert_eq!(group.reclaimable_bytes, 4); // size * (2 - 1)
        assert_eq!(result.reclaimable_bytes, 4);

        // Every file was hashed proactively at index time (not just the candidates).
        let hashed: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM files WHERE content_hash IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(hashed, 3);
    }

    #[test]
    fn hashes_files_proactively_at_index_time() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("Multimedia");
        fs::create_dir_all(root.join("Photo/DCIM")).unwrap();
        fs::write(root.join("Photo/DCIM/a.jpg"), [1, 2, 3]).unwrap();
        fs::write(root.join("Photo/DCIM/b.jpg"), [4, 5, 6, 7]).unwrap();

        let mut connection = Connection::open_in_memory().unwrap();
        index_multimedia(&mut connection, &root).unwrap();

        // Both files carry a content hash straight after indexing — no scan needed.
        let rows: Vec<(String, Option<String>)> = {
            let mut statement = connection
                .prepare("SELECT relative_path, content_hash FROM files ORDER BY relative_path")
                .unwrap();
            let mapped = statement
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
                .unwrap();
            mapped.map(|row| row.unwrap()).collect()
        };
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|(_, hash)| hash.is_some()));
        let expected_a = format!("{:x}", Sha256::digest([1, 2, 3]));
        assert_eq!(rows[0].1.as_deref(), Some(expected_a.as_str()));
    }

    #[test]
    fn groups_visually_similar_photos() {
        use image::{ImageBuffer, Luma};
        let temp = tempdir().unwrap();
        let root = temp.path().join("Photo");
        fs::create_dir_all(&root).unwrap();

        // Two horizontal gradients (near-identical dHash) + one vertical gradient.
        let a = ImageBuffer::from_fn(16, 16, |x, _y| Luma([(x as u8) * 16]));
        let b = ImageBuffer::from_fn(16, 16, |x, _y| Luma([(x as u8) * 16 + 3]));
        let c = ImageBuffer::from_fn(16, 16, |_x, y| Luma([(y as u8) * 16]));
        a.save(root.join("a.png")).unwrap();
        b.save(root.join("b.png")).unwrap();
        c.save(root.join("c.png")).unwrap();

        let mut connection = Connection::open_in_memory().unwrap();
        index_multimedia(&mut connection, &root).unwrap();

        let result = find_similar_photos(&connection, &mut |_done, _total| {}).unwrap();
        assert_eq!(result.scanned_candidates, 3);
        assert_eq!(result.total_groups, 1);
        let group = &result.groups[0];
        assert_eq!(group.files.len(), 2);
        let names: Vec<&str> = group.files.iter().map(|f| f.relative_path.as_str()).collect();
        assert!(names.contains(&"a.png") && names.contains(&"b.png"));

        // dHash was cached for every photo so the next scan is instant.
        let cached: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM files WHERE perceptual_hash IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(cached, 3);
    }

    #[test]
    fn refuses_to_trash_files_inside_app_storage() {
        let connection = Connection::open_in_memory().unwrap();
        initialize(&connection).unwrap();
        let app_root = default_app_dir().unwrap();
        let library_file = app_root
            .join("library/objects/ab/abc.jpg")
            .to_string_lossy()
            .into_owned();

        let result =
            move_files_to_trash_with_connection(&connection, vec![library_file]).unwrap();

        assert_eq!(result.trashed, 0);
        assert_eq!(result.removed_from_index, 0);
        assert_eq!(result.errors.len(), 1);
        assert!(result.errors[0].contains("own storage"));
    }

    #[test]
    fn initializes_versioned_schema() {
        let connection = Connection::open_in_memory().unwrap();
        initialize(&connection).unwrap();

        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION);

        let contents_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'contents'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(contents_count, 1);
    }
}
