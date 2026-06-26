use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::path::Path;

use super::indexing::hash_file_contents;
use super::schema::initialize;
use super::{
    indexed_file_from_row, open_default_connection, DbError, DuplicateGroup, DuplicateScanResult,
    IndexedFile,
};

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
