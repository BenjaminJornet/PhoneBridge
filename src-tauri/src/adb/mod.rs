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

            Some(BackupSource {
                id: Uuid::new_v5(&Uuid::NAMESPACE_URL, serial.as_bytes()).to_string(),
                adapter: "adb-generic".to_string(),
                label: format!("Android device {serial}"),
                path: None,
                device: Some(DeviceSummary {
                    id: serial.to_string(),
                    model: "Android device".to_string(),
                    manufacturer: "unknown".to_string(),
                    android_version: None,
                    connection: "adb".to_string(),
                }),
                created_at: None,
            })
        })
        .collect();

    Ok(devices)
}
