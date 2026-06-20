use crate::path_utils::expand_home;
use serde::{Deserialize, Serialize};
use std::fs;
use std::fs::File;
use std::path::{Path, PathBuf};
use tauri::{Emitter, Window};
use thiserror::Error;
use walkdir::WalkDir;

const SMARTSWITCH_MEDIA_CATEGORIES: [&str; 4] = ["Photo", "Video", "Music", "Documents"];

pub const WHATSAPP_MEDIA_MAPPING: [(&str, &str); 5] = [
    ("WhatsApp Images", "Photo"),
    ("WhatsApp Video", "Video"),
    ("WhatsApp Video Notes", "Video"),
    ("WhatsApp Audio", "Music"),
    ("WhatsApp Voice Notes", "Music"),
];

#[derive(Debug, Error)]
pub enum SyncError {
    #[error("filesystem error: {0}")]
    Filesystem(#[from] std::io::Error),
    #[error("walkdir error: {0}")]
    Walkdir(#[from] walkdir::Error),
    #[error("invalid source path: {0}")]
    InvalidSource(String),
    #[error("invalid destination path: {0}")]
    InvalidDestination(String),
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartSwitchCategory {
    pub name: String,
    pub source_path: String,
    pub file_count: u64,
    pub total_bytes: u64,
    pub sub_sources: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartSwitchSyncConfig {
    pub source_path: String,
    pub destination_path: String,
    pub categories: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartSwitchSyncResult {
    pub copied_files: u64,
    pub skipped_files: u64,
    pub copied_bytes: u64,
    pub skipped_bytes: u64,
    pub errors: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartSwitchSyncProgress {
    pub copied_files: u64,
    pub skipped_files: u64,
    pub current_path: String,
}

pub fn scan_smartswitch_categories(
    source_path: &Path,
) -> Result<Vec<SmartSwitchCategory>, SyncError> {
    if !source_path.is_dir() {
        return Err(SyncError::InvalidSource(
            source_path.to_string_lossy().into_owned(),
        ));
    }

    let mut categories = Vec::new();
    for category in SMARTSWITCH_MEDIA_CATEGORIES {
        let category_path = source_path.join(category);
        if !category_path.is_dir() {
            continue;
        }

        let mut file_count = 0;
        let mut total_bytes = 0;
        let mut sub_sources = Vec::new();

        for entry in WalkDir::new(&category_path).min_depth(1) {
            let entry = entry?;
            if entry.depth() == 1 && entry.file_type().is_dir() {
                sub_sources.push(entry.file_name().to_string_lossy().into_owned());
            }
            if entry.file_type().is_file() {
                file_count += 1;
                total_bytes += entry.metadata()?.len();
            }
        }

        sub_sources.sort();
        categories.push(SmartSwitchCategory {
            name: category.to_string(),
            source_path: category_path.to_string_lossy().into_owned(),
            file_count,
            total_bytes,
            sub_sources,
        });
    }

    Ok(categories)
}

#[cfg(test)]
fn execute_smartswitch_sync(
    config: SmartSwitchSyncConfig,
) -> Result<SmartSwitchSyncResult, SyncError> {
    execute_smartswitch_sync_with_progress(config, None)
}

pub fn execute_smartswitch_sync_with_window(
    config: SmartSwitchSyncConfig,
    window: Window,
) -> Result<SmartSwitchSyncResult, SyncError> {
    execute_smartswitch_sync_with_progress(config, Some(window))
}

fn execute_smartswitch_sync_with_progress(
    config: SmartSwitchSyncConfig,
    window: Option<Window>,
) -> Result<SmartSwitchSyncResult, SyncError> {
    let source_path = expand_home(&config.source_path);
    let destination_path = expand_home(&config.destination_path);

    if !source_path.is_dir() {
        return Err(SyncError::InvalidSource(config.source_path));
    }
    if destination_path.as_os_str().is_empty() {
        return Err(SyncError::InvalidDestination(config.destination_path));
    }

    fs::create_dir_all(&destination_path)?;
    let _lock = ImportLock::acquire(&destination_path)?;

    let mut result = SmartSwitchSyncResult {
        copied_files: 0,
        skipped_files: 0,
        copied_bytes: 0,
        skipped_bytes: 0,
        errors: Vec::new(),
    };

    for category in config.categories {
        if !SMARTSWITCH_MEDIA_CATEGORIES.contains(&category.as_str()) {
            result
                .errors
                .push(format!("Unsupported category: {category}"));
            continue;
        }

        let category_source = source_path.join(&category);
        if !category_source.is_dir() {
            result.errors.push(format!(
                "Missing category folder: {}",
                category_source.display()
            ));
            continue;
        }

        sync_category(
            &category_source,
            &destination_path.join(destination_category_for_source(&category_source, &category)),
            &mut result,
            window.as_ref(),
        )?;
    }

    Ok(result)
}

fn destination_category_for_source(category_source: &Path, fallback: &str) -> String {
    let source_name = category_source
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(fallback);
    WHATSAPP_MEDIA_MAPPING
        .iter()
        .find_map(|(label, category)| (*label == source_name).then(|| category.to_string()))
        .unwrap_or_else(|| fallback.to_string())
}

fn sync_category(
    category_source: &Path,
    category_destination: &Path,
    result: &mut SmartSwitchSyncResult,
    window: Option<&Window>,
) -> Result<(), SyncError> {
    for entry in WalkDir::new(category_source) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }

        let relative_path = entry
            .path()
            .strip_prefix(category_source)
            .unwrap_or(entry.path());
        let target = category_destination.join(relative_path);
        let source_metadata = entry.metadata()?;
        let source_size = source_metadata.len();

        if target.exists() {
            let target_size = target
                .metadata()
                .map(|metadata| metadata.len())
                .unwrap_or(0);
            result.skipped_files += 1;
            result.skipped_bytes += source_size;
            if target_size != source_size {
                result.errors.push(format!(
                    "Skipped existing file with different size: {}",
                    target.display()
                ));
            }
            emit_progress(window, result, entry.path());
            continue;
        }

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        atomic_copy(entry.path(), &target)?;
        result.copied_files += 1;
        result.copied_bytes += source_size;
        emit_progress(window, result, entry.path());
    }

    Ok(())
}

fn emit_progress(window: Option<&Window>, result: &SmartSwitchSyncResult, path: &Path) {
    if let Some(window) = window {
        let _ = window.emit(
            "smartswitch-sync-progress",
            SmartSwitchSyncProgress {
                copied_files: result.copied_files,
                skipped_files: result.skipped_files,
                current_path: path.to_string_lossy().into_owned(),
            },
        );
    }
}

struct ImportLock {
    path: PathBuf,
}

impl ImportLock {
    fn acquire(root: &Path) -> Result<Self, SyncError> {
        let path = root.join(".phonebridge-import.lock");
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(_) => Ok(Self { path }),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                Err(SyncError::InvalidDestination(format!(
                    "import already running at {}",
                    root.display()
                )))
            }
            Err(err) => Err(SyncError::Filesystem(err)),
        }
    }
}

impl Drop for ImportLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn atomic_copy(source: &Path, target: &Path) -> Result<(), SyncError> {
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
        .unwrap_or("phonebridge-copy");
    target.with_file_name(format!(".{file_name}.phonebridge.tmp"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn scans_categories_and_syncs_without_overwriting() {
        let temp = tempdir().unwrap();
        let backup = temp.path().join("SM-TEST_20250101010101");
        let destination = temp.path().join("Aggregated");
        fs::create_dir_all(backup.join("Photo/DCIM")).unwrap();
        fs::create_dir_all(backup.join("Video/Camera")).unwrap();
        fs::write(backup.join("Photo/DCIM/image.jpg"), [1, 2, 3]).unwrap();
        fs::write(backup.join("Video/Camera/video.mp4"), [1, 2, 3, 4]).unwrap();

        let categories = scan_smartswitch_categories(&backup).unwrap();
        assert_eq!(categories.len(), 2);
        assert_eq!(categories[0].name, "Photo");
        assert_eq!(categories[0].file_count, 1);
        assert_eq!(categories[0].total_bytes, 3);
        assert_eq!(categories[0].sub_sources, vec!["DCIM"]);

        let result = execute_smartswitch_sync(SmartSwitchSyncConfig {
            source_path: backup.to_string_lossy().into_owned(),
            destination_path: destination.to_string_lossy().into_owned(),
            categories: vec!["Photo".to_string(), "Video".to_string()],
        })
        .unwrap();

        assert_eq!(result.copied_files, 2);
        assert_eq!(result.skipped_files, 0);
        assert!(destination.join("Photo/DCIM/image.jpg").exists());
        assert!(destination.join("Video/Camera/video.mp4").exists());

        let second_result = execute_smartswitch_sync(SmartSwitchSyncConfig {
            source_path: backup.to_string_lossy().into_owned(),
            destination_path: destination.to_string_lossy().into_owned(),
            categories: vec!["Photo".to_string(), "Video".to_string()],
        })
        .unwrap();

        assert_eq!(second_result.copied_files, 0);
        assert_eq!(second_result.skipped_files, 2);
    }

    #[test]
    fn maps_whatsapp_audio_to_music() {
        assert_eq!(
            WHATSAPP_MEDIA_MAPPING
                .iter()
                .find(|(label, _)| *label == "WhatsApp Audio")
                .unwrap()
                .1,
            "Music"
        );
        assert_eq!(
            WHATSAPP_MEDIA_MAPPING
                .iter()
                .find(|(label, _)| *label == "WhatsApp Voice Notes")
                .unwrap()
                .1,
            "Music"
        );
    }

    #[test]
    fn atomic_copy_does_not_leave_temp_file_after_success() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("source.jpg");
        let target = temp.path().join("nested/target.jpg");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&source, [1, 2, 3, 4]).unwrap();

        atomic_copy(&source, &target).unwrap();

        assert_eq!(fs::read(&target).unwrap(), vec![1, 2, 3, 4]);
        assert!(!temporary_copy_path(&target).exists());
    }

    #[test]
    fn expands_home_prefix() {
        let expanded = expand_home("~/.phonebridge/library");
        assert!(expanded.ends_with(".phonebridge/library"));
        assert!(!expanded.to_string_lossy().starts_with("~"));
    }
}
