use crate::adapters::CategoryMetric;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use walkdir::WalkDir;

const APP_DIR_NAME: &str = ".phonebridge";
const DB_FILE_NAME: &str = "phonebridge.sqlite3";
const MULTIMEDIA_CATEGORIES: [&str; 4] = ["Photo", "Video", "Music", "Documents"];
const CURRENT_SCHEMA_VERSION: i64 = 3;

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

pub fn index_folder(source_path: String) -> Result<IndexSummary, DbError> {
    let root = expand_home(&source_path);
    let mut connection = open_default_connection()?;
    index_multimedia(&mut connection, &root)
}

fn expand_home(path: &str) -> PathBuf {
    if path == "~" {
        return env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(path));
    }

    if let Some(rest) = path.strip_prefix("~/") {
        return env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join(rest);
    }

    PathBuf::from(path)
}

pub fn get_indexed_category_metrics() -> Result<Vec<CategoryMetric>, DbError> {
    let connection = open_default_connection()?;
    category_metrics(&connection)
}

pub fn list_default_indexed_files(
    category: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<IndexedFile>, DbError> {
    let connection = open_default_connection()?;
    list_indexed_files(&connection, category.as_deref(), limit.unwrap_or(120))
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
              indexed_at
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
        )?;

        for entry in WalkDir::new(root) {
            let entry = entry?;
            if !entry.file_type().is_file() {
                continue;
            }

            let metadata = entry.metadata()?;
            let absolute_path = entry.path().to_string_lossy().into_owned();
            let relative_path = relative_path(root, entry.path());
            let category = category_from_relative_path(&relative_path);
            let source = source_from_relative_path(&relative_path);
            let extension = entry
                .path()
                .extension()
                .and_then(|value| value.to_str())
                .map(|value| value.to_ascii_lowercase());
            let modified_unix = metadata
                .modified()
                .ok()
                .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|value| value.as_secs() as i64);
            let size_bytes = metadata.len();

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
) -> Result<Vec<IndexedFile>, DbError> {
    initialize(connection)?;
    let safe_limit = limit.clamp(1, 500) as i64;

    let sql = if category.is_some() {
        "
        SELECT id, absolute_path, relative_path, category, source, extension, size_bytes, modified_unix
        FROM files
        WHERE category = ?1
        ORDER BY COALESCE(modified_unix, 0) DESC, size_bytes DESC
        LIMIT ?2
        "
    } else {
        "
        SELECT id, absolute_path, relative_path, category, source, extension, size_bytes, modified_unix
        FROM files
        ORDER BY COALESCE(modified_unix, 0) DESC, size_bytes DESC
        LIMIT ?1
        "
    };

    let mut statement = connection.prepare(sql)?;
    let rows = if let Some(category) = category {
        statement.query_map(params![category, safe_limit], indexed_file_from_row)?
    } else {
        statement.query_map(params![safe_limit], indexed_file_from_row)?
    };

    let mut files = Vec::new();
    for row in rows {
        files.push(row?);
    }

    Ok(files)
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

fn source_from_relative_path(path: &str) -> String {
    let mut parts = path.split(std::path::MAIN_SEPARATOR);
    let _category = parts.next();
    parts.next().unwrap_or("local").to_string()
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

        let files = list_indexed_files(&connection, Some("photo"), 10).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].relative_path, "Photo/DCIM/image.jpg");
        assert_eq!(files[0].source, "DCIM");
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
