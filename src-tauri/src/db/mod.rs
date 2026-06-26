use crate::adapters::CategoryMetric;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

mod classification;
mod duplicates;
mod indexing;
mod schema;
mod similarity;

pub use classification::classify_media_category;
pub use duplicates::find_default_duplicate_files;
pub use indexing::index_folder;
pub use schema::initialize;
pub use similarity::find_default_similar_photos;
// Re-exported to preserve the historic `crate::db::{...}` public paths after the module split.
// These are only consumed from other crate modules (or `#[cfg(test)]`), which the
// unused-imports lint does not see when building the lib, so silence the false positive.
#[allow(unused_imports)]
pub use duplicates::find_duplicate_files;
#[allow(unused_imports)]
pub use indexing::index_multimedia;
#[allow(unused_imports)]
pub use similarity::find_similar_photos;

use classification::{relative_path, source_from_relative_path};

pub(super) const APP_DIR_NAME: &str = ".phonebridge";
const DB_FILE_NAME: &str = "phonebridge.sqlite3";
pub(super) const THUMBS_DIR_NAME: &str = "thumbs";
const MULTIMEDIA_CATEGORIES: [&str; 4] = ["Photo", "Video", "Music", "Documents"];

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
    // Custom destinations: protect every consolidated library, wherever the user put it,
    // not just the default `~/.phonebridge`. Derived from `contents.storage_path`.
    let library_roots = protected_library_roots(connection);
    let mut errors = Vec::new();
    let mut safe_paths = Vec::new();

    for raw in paths {
        let path = PathBuf::from(&raw);
        if path_within(&path, &app_root)
            || library_roots.iter().any(|root| path_within(&path, root))
        {
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
        )?;
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

/// Distinct library `objects` directories recorded in `contents.storage_path`.
///
/// Every consolidated content object is stored at `{destination}/objects/{shard}/{file}`
/// (see `library::content_storage_path`). The destination can be anywhere the user chose,
/// so the safe-purge guardrail can't rely on the default app dir alone — it has to protect
/// each `objects` root the library actually lives in. Best-effort: a malformed row is skipped.
fn protected_library_roots(connection: &Connection) -> Vec<PathBuf> {
    let Ok(mut statement) = connection.prepare("SELECT DISTINCT storage_path FROM contents") else {
        return Vec::new();
    };
    let Ok(rows) = statement.query_map([], |row| row.get::<_, String>(0)) else {
        return Vec::new();
    };
    let mut roots: Vec<PathBuf> = Vec::new();
    for raw in rows.flatten() {
        let path = PathBuf::from(&raw);
        if let Some(objects_dir) = path
            .ancestors()
            .find(|ancestor| ancestor.file_name().map(|name| name == "objects").unwrap_or(false))
        {
            let objects_dir = objects_dir.to_path_buf();
            if !roots.contains(&objects_dir) {
                roots.push(objects_dir);
            }
        }
    }
    roots
}

/// Whether `candidate` is `root` or sits inside it. Uses a raw prefix check first (works for
/// not-yet-existing paths and matches the historic guardrail), then a canonicalized check to
/// defeat `..`/symlink tricks when both paths resolve on disk.
fn path_within(candidate: &Path, root: &Path) -> bool {
    if candidate.starts_with(root) {
        return true;
    }
    match (candidate.canonicalize(), root.canonicalize()) {
        (Ok(canonical_candidate), Ok(canonical_root)) => {
            canonical_candidate.starts_with(canonical_root)
        }
        _ => false,
    }
}

pub(super) fn indexed_file_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<IndexedFile> {
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

pub(super) fn default_database_path() -> Result<PathBuf, DbError> {
    Ok(env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(APP_DIR_NAME)
        .join(DB_FILE_NAME))
}

#[cfg(test)]
mod tests {
    use super::schema::CURRENT_SCHEMA_VERSION;
    use super::similarity::PERCEPTUAL_SIGNATURE_LEN;
    use super::*;
    use sha2::{Digest, Sha256};
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

        // The 256-bit signature was cached for every photo so the next scan is instant.
        let cached: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM files WHERE perceptual_hash_v2 IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(cached, 3);
    }

    #[test]
    fn computes_perceptual_signature_at_index_time() {
        use image::{ImageBuffer, Luma};
        let temp = tempdir().unwrap();
        let root = temp.path().join("Photo");
        fs::create_dir_all(&root).unwrap();
        let gradient = ImageBuffer::from_fn(16, 16, |x, _y| Luma([(x as u8) * 16]));
        gradient.save(root.join("a.png")).unwrap();

        let mut connection = Connection::open_in_memory().unwrap();
        index_multimedia(&mut connection, &root).unwrap();

        // The signature is populated by indexing alone — no near-dup scan required.
        let signature: Option<Vec<u8>> = connection
            .query_row(
                "SELECT perceptual_hash_v2 FROM files WHERE relative_path = 'a.png'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(signature.map(|bytes| bytes.len()), Some(PERCEPTUAL_SIGNATURE_LEN));
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
    fn refuses_to_trash_files_inside_custom_library_destination() {
        let connection = Connection::open_in_memory().unwrap();
        initialize(&connection).unwrap();
        // A library consolidated to a non-default destination records its objects here.
        let custom_object = "/Volumes/Archive/phonebridge/objects/ab/abc.jpg";
        connection
            .execute(
                "INSERT INTO contents (hash, size_bytes, extension, storage_path, first_seen_at)
                 VALUES ('abc', 10, 'jpg', ?1, CURRENT_TIMESTAMP)",
                params![custom_object],
            )
            .unwrap();

        let result =
            move_files_to_trash_with_connection(&connection, vec![custom_object.to_string()])
                .unwrap();

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
