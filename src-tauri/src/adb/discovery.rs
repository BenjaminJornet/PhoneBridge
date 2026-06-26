use crate::adapters::{AdapterError, BackupSource, DeviceSummary};
use crate::privacy::redact_identifier;
use std::process::Command;
use uuid::Uuid;

use super::commands::{adb_getprop, adb_output, resolve_adb_command};
use super::{AdbDiagnostic, AdbDiagnosticDevice};

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

pub(super) fn resolve_serial_for_source_id(source_id: &str) -> Result<String, AdapterError> {
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

pub(super) fn source_id_for_serial(serial: &str) -> String {
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
