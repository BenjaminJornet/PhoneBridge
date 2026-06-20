use crate::adapters::{AdapterError, BackupSource, DeviceSummary};
use crate::privacy::redact_identifier;
use serde::Serialize;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{Emitter, Window};
use uuid::Uuid;

const ADB_MEDIA_PATHS: [(&str, &str); 6] = [
    ("DCIM", "/sdcard/DCIM/"),
    ("Pictures", "/sdcard/Pictures/"),
    ("Download", "/sdcard/Download/"),
    ("Movies", "/sdcard/Movies/"),
    ("Music", "/sdcard/Music/"),
    (
        "WhatsApp Media",
        "/storage/emulated/0/Android/media/com.whatsapp/WhatsApp/Media/",
    ),
];

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdbPullProgress {
    pub pulled_paths: u64,
    pub skipped_paths: u64,
    pub pulled_files: u64,
    pub skipped_files: u64,
    pub total_files: u64,
    pub current_path: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdbMediaFolderPreview {
    pub key: String,
    pub label: String,
    pub remote_path: String,
    pub file_count: u64,
    pub total_bytes: u64,
    pub available: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdbPullResult {
    pub source_path: String,
    pub pulled_paths: u64,
    pub skipped_paths: u64,
    pub pulled_files: u64,
    pub skipped_files: u64,
    pub total_files: u64,
    pub errors: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdbDiagnosticDevice {
    pub source_id: String,
    pub label: String,
    pub status: String,
    pub model: Option<String>,
    pub manufacturer: Option<String>,
    pub android_version: Option<String>,
    pub redacted_id: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdbDiagnostic {
    pub adb_found: bool,
    pub adb_path: Option<String>,
    pub devices: Vec<AdbDiagnosticDevice>,
    pub message: String,
    pub next_action: String,
}

#[derive(Clone, Debug)]
struct AdbDeviceRow {
    serial: String,
    status: String,
}

pub fn detect_devices() -> Result<Vec<BackupSource>, AdapterError> {
    let devices = parse_adb_devices(&adb_output(&["devices"])?);
    let sources = devices
        .into_iter()
        .filter_map(|device| {
            if device.status != "device" {
                return None;
            }

            let model = adb_getprop(&device.serial, "ro.product.model")
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "Android device".to_string());
            let manufacturer = adb_getprop(&device.serial, "ro.product.manufacturer")
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "unknown".to_string());
            let android_version = adb_getprop(&device.serial, "ro.build.version.release")
                .filter(|value| !value.is_empty());

            Some(BackupSource {
                id: source_id_for_serial(&device.serial),
                adapter: "adb-generic".to_string(),
                label: format!("{manufacturer} {model}"),
                path: None,
                device: Some(DeviceSummary {
                    id: redact_identifier(&device.serial),
                    model,
                    manufacturer,
                    android_version,
                    connection: "adb".to_string(),
                }),
                created_at: None,
            })
        })
        .collect();

    Ok(sources)
}

pub fn diagnose_adb() -> AdbDiagnostic {
    let Ok(adb_path) = resolve_adb_command() else {
        return AdbDiagnostic {
            adb_found: false,
            adb_path: None,
            devices: Vec::new(),
            message: "ADB was not found from the app environment.".to_string(),
            next_action: "Install Android Platform Tools, or set ADB_PATH to the adb binary path."
                .to_string(),
        };
    };

    let output = Command::new(&adb_path).arg("devices").output();
    let Ok(output) = output else {
        return AdbDiagnostic {
            adb_found: true,
            adb_path: Some(adb_path.to_string_lossy().into_owned()),
            devices: Vec::new(),
            message: "ADB was found, but PhoneBridge could not run `adb devices`.".to_string(),
            next_action: "Try reconnecting the phone, then refresh devices.".to_string(),
        };
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return AdbDiagnostic {
            adb_found: true,
            adb_path: Some(adb_path.to_string_lossy().into_owned()),
            devices: Vec::new(),
            message: if stderr.is_empty() {
                "`adb devices` failed.".to_string()
            } else {
                stderr
            },
            next_action: "Restart ADB or reconnect the phone, then refresh devices.".to_string(),
        };
    }

    let rows = parse_adb_devices(&String::from_utf8_lossy(&output.stdout));
    let devices: Vec<_> = rows
        .into_iter()
        .map(|device| {
            let model =
                adb_getprop(&device.serial, "ro.product.model").filter(|value| !value.is_empty());
            let manufacturer = adb_getprop(&device.serial, "ro.product.manufacturer")
                .filter(|value| !value.is_empty());
            let android_version = adb_getprop(&device.serial, "ro.build.version.release")
                .filter(|value| !value.is_empty());
            let label = match (&manufacturer, &model) {
                (Some(manufacturer), Some(model)) => format!("{manufacturer} {model}"),
                (_, Some(model)) => model.clone(),
                _ => "Android device".to_string(),
            };
            AdbDiagnosticDevice {
                source_id: source_id_for_serial(&device.serial),
                label,
                status: device.status,
                model,
                manufacturer,
                android_version,
                redacted_id: redact_identifier(&device.serial),
            }
        })
        .collect();

    let authorized = devices
        .iter()
        .filter(|device| device.status == "device")
        .count();
    let (message, next_action) = if authorized > 0 {
        (
            format!("{authorized} authorized Android device(s) ready."),
            "Select the Android phone source, then copy media when you are ready.".to_string(),
        )
    } else if devices.iter().any(|device| device.status == "unauthorized") {
        (
            "A phone is connected but not authorized for USB debugging.".to_string(),
            "Unlock the phone and accept the USB debugging prompt, then refresh devices."
                .to_string(),
        )
    } else if devices.iter().any(|device| device.status == "offline") {
        (
            "A phone is connected but ADB reports it as offline.".to_string(),
            "Reconnect the cable or toggle USB debugging, then refresh devices.".to_string(),
        )
    } else {
        (
            "ADB is installed, but no Android phone is connected.".to_string(),
            "Connect a phone with USB debugging enabled, then refresh devices.".to_string(),
        )
    };

    AdbDiagnostic {
        adb_found: true,
        adb_path: Some(adb_path.to_string_lossy().into_owned()),
        devices,
        message,
        next_action,
    }
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

/// Run `adb -s <serial> shell <command>` and return stdout, tolerating non-zero exits
/// (find/du report failures for unreadable subtrees we deliberately skip).
fn adb_shell_capture(serial: &str, command: &str) -> String {
    let Ok(adb) = resolve_adb_command() else {
        return String::new();
    };
    Command::new(adb)
        .args(["-s", serial, "shell", command])
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default()
}

pub fn pull_device_media_by_source_id(
    source_id: &str,
    staging_root: &Path,
    selected_keys: Option<Vec<String>>,
    window: Option<Window>,
) -> Result<AdbPullResult, AdapterError> {
    let serial = resolve_serial_for_source_id(source_id)?;
    let source_path = staging_root.join(source_id);
    pull_device_media(&serial, &source_path, selected_keys.as_deref(), window)
}

fn resolve_serial_for_source_id(source_id: &str) -> Result<String, AdapterError> {
    let output = adb_output(&["devices"])?;
    for line in output.lines().skip(1) {
        let mut parts = line.split_whitespace();
        let Some(serial) = parts.next() else {
            continue;
        };
        let Some(status) = parts.next() else {
            continue;
        };
        if status == "device" && source_id_for_serial(serial) == source_id {
            return Ok(serial.to_string());
        }
    }
    Err(AdapterError::CommandFailed(format!(
        "adb device not found for source {source_id}"
    )))
}

pub fn pull_device_media(
    serial: &str,
    source_path: &Path,
    selected_keys: Option<&[String]>,
    window: Option<Window>,
) -> Result<AdbPullResult, AdapterError> {
    fs::create_dir_all(source_path)?;
    let mut result = AdbPullResult {
        source_path: source_path.to_string_lossy().into_owned(),
        pulled_paths: 0,
        skipped_paths: 0,
        pulled_files: 0,
        skipped_files: 0,
        total_files: 0,
        errors: Vec::new(),
    };

    for (label, remote_path) in ADB_MEDIA_PATHS {
        // When a selection is provided, only pull the chosen folders (avoids a blind
        // multi-gigabyte copy of every default media path).
        if let Some(keys) = selected_keys {
            if !keys.iter().any(|key| key == label) {
                continue;
            }
        }

        if let Ok(files) = list_remote_files(serial, remote_path) {
            result.total_files += files.len() as u64;
            let local_root = source_path.join(label);
            for remote_file in files {
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

fn list_remote_files(serial: &str, remote_path: &str) -> Result<Vec<String>, AdapterError> {
    let output = adb_output(&["-s", serial, "shell", "find", remote_path, "-type", "f"])?;
    let files: Vec<String> = output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.contains("Permission denied"))
        .map(ToString::to_string)
        .collect();
    if files.is_empty() {
        return Err(AdapterError::CommandFailed(format!(
            "adb find found no files at {remote_path}"
        )));
    }
    Ok(files)
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

fn adb_pull(serial: &str, remote_path: &str, local_path: &Path) -> Result<(), AdapterError> {
    if let Some(parent) = local_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let output = Command::new(resolve_adb_command()?)
        .args(["-s", serial, "pull", remote_path])
        .arg(local_path)
        .output();
    let Ok(output) = output else {
        return Err(AdapterError::CommandUnavailable("adb".to_string()));
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(AdapterError::CommandFailed(format!(
            "adb pull {remote_path}: {stderr}"
        )));
    }
    Ok(())
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
                total_files: result.total_files,
                current_path: current_path.to_string(),
            },
        );
    }
}

fn adb_getprop(serial: &str, property: &str) -> Option<String> {
    adb_output(&["-s", serial, "shell", "getprop", property])
        .ok()
        .map(|value| value.trim().to_string())
}

fn adb_output(args: &[&str]) -> Result<String, AdapterError> {
    let output = Command::new(resolve_adb_command()?).args(args).output();
    let Ok(output) = output else {
        return Err(AdapterError::CommandUnavailable("adb".to_string()));
    };
    if !output.status.success() {
        return Err(AdapterError::CommandFailed(format!(
            "adb {}",
            args.join(" ")
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn resolve_adb_command() -> Result<PathBuf, AdapterError> {
    let mut candidates = Vec::new();
    if let Some(path) = env::var_os("ADB_PATH").filter(|value| !value.is_empty()) {
        candidates.push(PathBuf::from(path));
    }
    candidates.push(PathBuf::from("adb"));
    candidates.push(PathBuf::from("/opt/homebrew/bin/adb"));
    candidates.push(PathBuf::from("/usr/local/bin/adb"));
    for env_name in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Some(root) = env::var_os(env_name).filter(|value| !value.is_empty()) {
            candidates.push(PathBuf::from(root).join("platform-tools/adb"));
        }
    }

    for candidate in candidates {
        if adb_candidate_works(&candidate) {
            return Ok(candidate);
        }
    }

    Err(AdapterError::CommandUnavailable("adb".to_string()))
}

fn adb_candidate_works(candidate: &Path) -> bool {
    Command::new(candidate)
        .arg("version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn parse_adb_devices(output: &str) -> Vec<AdbDeviceRow> {
    output
        .lines()
        .skip(1)
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let serial = parts.next()?;
            let status = parts.next()?;
            Some(AdbDeviceRow {
                serial: serial.to_string(),
                status: status.to_string(),
            })
        })
        .collect()
}

fn source_id_for_serial(serial: &str) -> String {
    Uuid::new_v5(&Uuid::NAMESPACE_URL, serial.as_bytes()).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adb_property_fallbacks_keep_source_shape_stable() {
        let source = BackupSource {
            id: Uuid::new_v5(&Uuid::NAMESPACE_URL, b"SERIAL").to_string(),
            adapter: "adb-generic".to_string(),
            label: "unknown Android device".to_string(),
            path: None,
            device: Some(DeviceSummary {
                id: "SERIAL".to_string(),
                model: "Android device".to_string(),
                manufacturer: "unknown".to_string(),
                android_version: None,
                connection: "adb".to_string(),
            }),
            created_at: None,
        };

        assert_eq!(source.adapter, "adb-generic");
        assert_eq!(source.device.unwrap().connection, "adb");
    }

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
    fn parses_adb_device_statuses() {
        let rows = parse_adb_devices(
            "List of devices attached\nSERIAL1\tdevice\nSERIAL2\tunauthorized\nSERIAL3\toffline\n",
        );

        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].status, "device");
        assert_eq!(rows[1].status, "unauthorized");
        assert_eq!(rows[2].status, "offline");
    }

    #[test]
    fn adb_source_ids_are_stable() {
        assert_eq!(
            source_id_for_serial("SERIAL"),
            source_id_for_serial("SERIAL")
        );
        assert_ne!(
            source_id_for_serial("SERIAL"),
            source_id_for_serial("OTHER")
        );
    }
}
