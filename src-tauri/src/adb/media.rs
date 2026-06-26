use crate::adapters::AdapterError;
use std::fs;
use std::path::Path;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tauri::{Emitter, Window};

use super::commands::{adb_output, adb_pull, adb_shell_capture};
use super::discovery::resolve_serial_for_source_id;
use super::{AdbMediaFolderPreview, AdbPullProgress, AdbPullResult, ADB_MEDIA_PATHS};

#[derive(Debug, Default)]
struct AdbFindResult {
    files: Vec<String>,
    permission_denied_files: u64,
}

pub fn preview_device_media_by_source_id(
    source_id: &str,
) -> Result<Vec<AdbMediaFolderPreview>, AdapterError> {
    let serial = resolve_serial_for_source_id(source_id)?;
    let previews = ADB_MEDIA_PATHS
        .iter()
        .map(|(label, remote_path)| {
            let file_count = adb_shell_count(&serial, remote_path);
            let total_bytes = adb_shell_size_bytes(&serial, remote_path);
            AdbMediaFolderPreview {
                key: (*label).to_string(),
                label: (*label).to_string(),
                remote_path: (*remote_path).to_string(),
                file_count,
                total_bytes,
                available: file_count > 0,
            }
        })
        .collect();
    Ok(previews)
}

/// Count regular files under a remote path, ignoring permission errors. Best effort:
/// returns 0 when the path is missing or unreadable rather than failing the whole preview.
fn adb_shell_count(serial: &str, remote_path: &str) -> u64 {
    let command = format!("find '{remote_path}' -type f 2>/dev/null | wc -l");
    adb_shell_capture(serial, &command)
        .trim()
        .parse()
        .unwrap_or(0)
}

/// Total size in bytes of a remote path (`du -sk` reports kibibytes), best effort.
fn adb_shell_size_bytes(serial: &str, remote_path: &str) -> u64 {
    let command = format!("du -sk '{remote_path}' 2>/dev/null | tail -1");
    let kib: u64 = adb_shell_capture(serial, &command)
        .split_whitespace()
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    kib.saturating_mul(1024)
}

pub fn pull_device_media_by_source_id(
    source_id: &str,
    staging_root: &Path,
    selected_keys: Option<Vec<String>>,
    window: Option<Window>,
    cancel_token: Arc<AtomicBool>,
) -> Result<AdbPullResult, AdapterError> {
    let serial = resolve_serial_for_source_id(source_id)?;
    let source_path = staging_root.join(source_id);
    pull_device_media(&serial, &source_path, selected_keys.as_deref(), window, cancel_token)
}

pub fn pull_device_media(
    serial: &str,
    source_path: &Path,
    selected_keys: Option<&[String]>,
    window: Option<Window>,
    cancel_token: Arc<AtomicBool>,
) -> Result<AdbPullResult, AdapterError> {
    fs::create_dir_all(source_path)?;
    let mut result = AdbPullResult {
        source_path: source_path.to_string_lossy().into_owned(),
        pulled_paths: 0,
        skipped_paths: 0,
        pulled_files: 0,
        skipped_files: 0,
        permission_denied_files: 0,
        total_files: 0,
        errors: Vec::new(),
        cancelled: false,
    };

    for (label, remote_path) in ADB_MEDIA_PATHS {
        if cancel_token.load(Ordering::Relaxed) {
            result.cancelled = true;
            break;
        }

        // When a selection is provided, only pull the chosen folders (avoids a blind
        // multi-gigabyte copy of every default media path).
        if let Some(keys) = selected_keys {
            if !keys.iter().any(|key| key == label) {
                continue;
            }
        }

        if let Ok(find_result) = list_remote_files(serial, remote_path) {
            result.total_files += find_result.files.len() as u64;
            result.permission_denied_files += find_result.permission_denied_files;
            let local_root = source_path.join(label);
            for remote_file in find_result.files {
                if cancel_token.load(Ordering::Relaxed) {
                    result.cancelled = true;
                    return Ok(result);
                }
                emit_pull_progress(window.as_ref(), &result, &remote_file);
                match pull_remote_file(serial, remote_path, &remote_file, &local_root) {
                    Ok(()) => result.pulled_files += 1,
                    Err(err) => {
                        result.skipped_files += 1;
                        result.errors.push(err.to_string());
                    }
                }
            }
            result.pulled_paths += 1;
            emit_pull_progress(window.as_ref(), &result, remote_path);
            continue;
        }

        emit_pull_progress(window.as_ref(), &result, remote_path);
        let local_path = source_path.join(label);
        match adb_pull(serial, remote_path, &local_path) {
            Ok(()) => {
                result.pulled_paths += 1;
            }
            Err(err) => {
                result.skipped_paths += 1;
                result.errors.push(err.to_string());
            }
        }
        emit_pull_progress(window.as_ref(), &result, remote_path);
    }

    Ok(result)
}

fn list_remote_files(serial: &str, remote_path: &str) -> Result<AdbFindResult, AdapterError> {
    let output = adb_output(&["-s", serial, "shell", "find", remote_path, "-type", "f"])?;
    let find_result = parse_find_output(&output);
    if find_result.files.is_empty() {
        return Err(AdapterError::CommandFailed(format!(
            "adb find found no files at {remote_path}"
        )));
    }
    Ok(find_result)
}

fn pull_remote_file(
    serial: &str,
    remote_root: &str,
    remote_file: &str,
    local_root: &Path,
) -> Result<(), AdapterError> {
    let relative = remote_file
        .trim_start_matches(remote_root.trim_end_matches('/'))
        .trim_start_matches('/');
    let local_path = local_root.join(relative);
    adb_pull(serial, remote_file, &local_path)
}

fn emit_pull_progress(window: Option<&Window>, result: &AdbPullResult, current_path: &str) {
    if let Some(window) = window {
        let _ = window.emit(
            "adb-pull-progress",
            AdbPullProgress {
                pulled_paths: result.pulled_paths,
                skipped_paths: result.skipped_paths,
                pulled_files: result.pulled_files,
                skipped_files: result.skipped_files,
                permission_denied_files: result.permission_denied_files,
                total_files: result.total_files,
                current_path: current_path.to_string(),
            },
        );
    }
}

fn parse_find_output(output: &str) -> AdbFindResult {
    let mut find_result = AdbFindResult::default();
    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if line.contains("Permission denied") {
            find_result.permission_denied_files += 1;
            continue;
        }
        find_result.files.push(line.to_string());
    }
    find_result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_pull_paths_include_common_android_folders() {
        let labels: Vec<_> = ADB_MEDIA_PATHS.iter().map(|(label, _)| *label).collect();
        assert!(labels.contains(&"DCIM"));
        assert!(labels.contains(&"Pictures"));
        assert!(labels.contains(&"Download"));
        assert!(labels.contains(&"Movies"));
        assert!(labels.contains(&"Music"));
        assert!(labels.contains(&"WhatsApp Media"));
    }

    #[test]
    fn counts_permission_denied_find_lines_without_treating_them_as_files() {
        let parsed = parse_find_output(
            "/sdcard/DCIM/photo.jpg\nfind: '/sdcard/Android/data': Permission denied\n",
        );

        assert_eq!(parsed.files, vec!["/sdcard/DCIM/photo.jpg"]);
        assert_eq!(parsed.permission_denied_files, 1);
    }
}
