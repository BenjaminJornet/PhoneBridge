use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use std::fs;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use super::{HashedFile, LibraryError};

pub(super) fn hash_file(path: &Path) -> Result<String, LibraryError> {
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

pub(super) fn content_exists(connection: &Connection, hash: &str) -> Result<bool, LibraryError> {
    Ok(connection
        .query_row(
            "SELECT 1 FROM contents WHERE hash = ?1",
            params![hash],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

pub(super) fn content_exists_in_transaction(
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

pub(super) fn content_storage_path(
    destination_path: &Path,
    hash: &str,
    extension: Option<&str>,
) -> PathBuf {
    let shard = &hash[..2];
    let file_name = if let Some(extension) = extension.filter(|value| !value.is_empty()) {
        format!("{hash}.{extension}")
    } else {
        hash.to_string()
    };
    destination_path.join("objects").join(shard).join(file_name)
}

pub(super) fn copy_content(file: &HashedFile) -> Result<(), LibraryError> {
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
