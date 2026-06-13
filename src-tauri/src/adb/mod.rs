use crate::adapters::{AdapterError, BackupSource, DeviceSummary};
use crate::privacy::redact_identifier;
use serde::Serialize;
use std::fs;
use std::path::Path;
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
    pub current_path: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdbPullResult {
    pub source_path: String,
    pub pulled_paths: u64,
    pub skipped_paths: u64,
    pub errors: Vec<String>,
}

pub fn detect_devices() -> Result<Vec<BackupSource>, AdapterError> {
    let output = Command::new("adb").arg("devices").output();
    let Ok(output) = output else {
        return Err(AdapterError::CommandUnavailable("adb".to_string()));
    };

    if !output.status.success() {
        return Err(AdapterError::CommandFailed("adb devices".to_string()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let devices = stdout
        .lines()
        .skip(1)
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let serial = parts.next()?;
            let status = parts.next()?;
            if status != "device" {
                return None;
            }

            let model = adb_getprop(serial, "ro.product.model")
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "Android device".to_string());
            let manufacturer = adb_getprop(serial, "ro.product.manufacturer")
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "unknown".to_string());
            let android_version =
                adb_getprop(serial, "ro.build.version.release").filter(|value| !value.is_empty());

            Some(BackupSource {
                id: Uuid::new_v5(&Uuid::NAMESPACE_URL, serial.as_bytes()).to_string(),
                adapter: "adb-generic".to_string(),
                label: format!("{manufacturer} {model}"),
                path: None,
                device: Some(DeviceSummary {
                    id: redact_identifier(serial),
                    model,
                    manufacturer,
                    android_version,
                    connection: "adb".to_string(),
                }),
                created_at: None,
            })
        })
        .collect();

    Ok(devices)
}

pub fn pull_device_media_by_source_id(
    source_id: &str,
    staging_root: &Path,
    window: Option<Window>,
) -> Result<AdbPullResult, AdapterError> {
    let serial = resolve_serial_for_source_id(source_id)?;
    let source_path = staging_root.join(source_id);
    pull_device_media(&serial, &source_path, window)
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
        if status == "device"
            && Uuid::new_v5(&Uuid::NAMESPACE_URL, serial.as_bytes()).to_string() == source_id
        {
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
    window: Option<Window>,
) -> Result<AdbPullResult, AdapterError> {
    fs::create_dir_all(source_path)?;
    let mut result = AdbPullResult {
        source_path: source_path.to_string_lossy().into_owned(),
        pulled_paths: 0,
        skipped_paths: 0,
        errors: Vec::new(),
    };

    for (label, remote_path) in ADB_MEDIA_PATHS {
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

fn adb_pull(serial: &str, remote_path: &str, local_path: &Path) -> Result<(), AdapterError> {
    if let Some(parent) = local_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let output = Command::new("adb")
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
    let output = Command::new("adb").args(args).output();
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
}
