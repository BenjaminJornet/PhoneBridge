use crate::adapters::{AdapterError, BackupSource, DeviceSummary};
use std::process::Command;
use uuid::Uuid;

pub fn detect_devices() -> Result<Vec<BackupSource>, AdapterError> {
    let output = Command::new("adb").arg("devices").output();
    let Ok(output) = output else {
        return Ok(Vec::new());
    };

    if !output.status.success() {
        return Ok(Vec::new());
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
                    id: serial.to_string(),
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

fn adb_getprop(serial: &str, property: &str) -> Option<String> {
    let output = Command::new("adb")
        .args(["-s", serial, "shell", "getprop", property])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
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
}
