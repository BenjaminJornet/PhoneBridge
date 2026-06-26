use crate::path_utils::expand_home;
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use super::schema::initialize;
use super::similarity::{perceptual_hash_256_of, IMAGE_DECODABLE_EXTENSIONS};
use super::{
    classify_media_category, default_database_path, open_default_connection, relative_path,
    source_from_relative_path, DbError, IndexSummary, APP_DIR_NAME, THUMBS_DIR_NAME,
};

// Photo extensions that WebViews can't render natively → generate JPEG thumbnails via sips.
const NON_WEB_PHOTO_EXTENSIONS: &[&str] = &[
    "heic", "heif", "tif", "tiff", "dng", "raw", "cr2", "nef", "arw", "rw2", "orf",
];

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
    type ReusableHashes = (i64, Option<i64>, Option<String>, Option<Vec<u8>>);
    let mut existing_hashes: HashMap<String, ReusableHashes> = HashMap::new();
    {
        let mut statement = connection.prepare(
            "SELECT absolute_path, size_bytes, modified_unix, content_hash, perceptual_hash_v2 FROM files",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<i64>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<Vec<u8>>>(4)?,
            ))
        })?;
        for row in rows {
            let (path, size, mtime, hash, perceptual) = row?;
            existing_hashes.insert(path, (size, mtime, hash, perceptual));
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
              perceptual_hash_v2,
              indexed_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, CURRENT_TIMESTAMP)
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
              perceptual_hash_v2 = excluded.perceptual_hash_v2,
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

            let unchanged = existing_hashes
                .get(&absolute_path)
                .map(|(prev_size, prev_mtime, _, _)| {
                    *prev_size == size_bytes as i64 && *prev_mtime == modified_unix
                })
                .unwrap_or(false);

            // Reuse the stored hash if the file is byte-for-byte unchanged; otherwise
            // hash it now. A read failure (permissions, vanished file) leaves the hash
            // NULL and the file is simply hashed lazily by the duplicate scanner later.
            let content_hash = match existing_hashes.get(&absolute_path) {
                Some((_, _, Some(prev_hash), _)) if unchanged => Some(prev_hash.clone()),
                _ => hash_file_contents(entry.path()).ok(),
            };

            // Compute the perceptual signature for photos up front (mirrors content_hash) so
            // the first near-duplicate scan is instant instead of decoding every image. Reuse
            // the cached signature for unchanged photos; decode the thumbnail (HEIC/RAW) or the
            // original otherwise.
            let perceptual_hash_v2: Option<Vec<u8>> = if category == "photo" {
                match existing_hashes.get(&absolute_path) {
                    Some((_, _, _, Some(prev_signature))) if unchanged => {
                        Some(prev_signature.clone())
                    }
                    _ => {
                        let decode_path = thumbnail_path.as_deref().map(PathBuf::from).or_else(|| {
                            extension
                                .as_deref()
                                .filter(|ext| IMAGE_DECODABLE_EXTENSIONS.contains(ext))
                                .map(|_| entry.path().to_path_buf())
                        });
                        decode_path
                            .as_deref()
                            .and_then(perceptual_hash_256_of)
                            .map(|signature| signature.to_vec())
                    }
                }
            } else {
                None
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
                perceptual_hash_v2,
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

/// Stream a file through SHA-256 in 64 KiB chunks. Kept local to the `db` module
/// (rather than reusing `library::hash_file`) so the lower-level `db` layer does
/// not depend on the higher-level `library` consolidation module.
pub(super) fn hash_file_contents(path: &Path) -> Result<String, DbError> {
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
