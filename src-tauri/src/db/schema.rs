use rusqlite::{params, Connection};

use super::classify_media_category;
use super::DbError;

pub(super) const CURRENT_SCHEMA_VERSION: i64 = 9;

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
        if version < 9 {
            migrate_to_v9(connection)?;
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

fn migrate_to_v9(connection: &Connection) -> Result<(), DbError> {
    // Store a 256-bit perceptual signature (16x16 dHash + aspect byte) per photo. The old
    // 64-bit `perceptual_hash` column is too coarse and is left in place but unused; this
    // BLOB column supersedes it for the near-duplicate finder.
    connection
        .execute_batch(
            "
        ALTER TABLE files ADD COLUMN perceptual_hash_v2 BLOB;
        PRAGMA user_version = 9;
        ",
        )
        .or_else(|err| {
            if err.to_string().contains("duplicate column name") {
                connection.execute_batch("PRAGMA user_version = 9;")?;
                Ok(())
            } else {
                Err(err)
            }
        })?;
    Ok(())
}
