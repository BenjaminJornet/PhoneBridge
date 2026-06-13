use super::{samsung_root, AdapterError, BackupAdapter, BackupSource, DeviceSummary};
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Default)]
pub struct SmartSwitchAdapter;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SmartSwitchMetadata {
    model_name: Option<String>,
    display_name: Option<String>,
    vendor: Option<String>,
    #[serde(rename = "PlatformVersion")]
    platform_version: Option<String>,
}

impl BackupAdapter for SmartSwitchAdapter {
    fn scan(&self) -> Result<Vec<BackupSource>, AdapterError> {
        let backup_root = samsung_root().join("SmartSwitch").join("Backup");
        if !backup_root.exists() {
            return Ok(Vec::new());
        }

        let mut sources = Vec::new();
        for entry in fs::read_dir(backup_root)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_dir() || !is_samsung_backup_dir(&path) {
                continue;
            }

            let metadata = read_metadata(&path)?;
            let label = metadata
                .as_ref()
                .and_then(|item| item.display_name.clone())
                .unwrap_or_else(|| entry.file_name().to_string_lossy().into_owned());

            let device = metadata.map(|item| DeviceSummary {
                id: entry.file_name().to_string_lossy().into_owned(),
                model: item
                    .model_name
                    .unwrap_or_else(|| "Unknown Android".to_string()),
                manufacturer: item.vendor.unwrap_or_else(|| "samsung".to_string()),
                android_version: item.platform_version,
                connection: "backup".to_string(),
            });

            sources.push(BackupSource {
                id: stable_source_id(&path),
                adapter: "samsung-smartswitch".to_string(),
                label,
                path: Some(path.to_string_lossy().into_owned()),
                device,
                created_at: timestamp_from_name(&entry.file_name().to_string_lossy()),
            });
        }

        sources.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        Ok(sources)
    }
}

fn is_samsung_backup_dir(path: &PathBuf) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("SM-"))
}

fn read_metadata(path: &PathBuf) -> Result<Option<SmartSwitchMetadata>, AdapterError> {
    let metadata_path = path.join("SmartSwitchBackup.json");
    if !metadata_path.exists() {
        return Ok(None);
    }

    let raw = fs::read_to_string(metadata_path)?;
    Ok(Some(serde_json::from_str(&raw)?))
}

fn stable_source_id(path: &PathBuf) -> String {
    Uuid::new_v5(&Uuid::NAMESPACE_URL, path.to_string_lossy().as_bytes()).to_string()
}

fn timestamp_from_name(name: &str) -> Option<String> {
    let raw = name.rsplit_once('_')?.1;
    if raw.len() != 14 || !raw.chars().all(|item| item.is_ascii_digit()) {
        return None;
    }

    Some(format!(
        "{}-{}-{}T{}:{}:{}",
        &raw[0..4],
        &raw[4..6],
        &raw[6..8],
        &raw[8..10],
        &raw[10..12],
        &raw[12..14]
    ))
}

#[cfg(test)]
mod tests {
    use super::timestamp_from_name;

    #[test]
    fn parses_samsung_backup_timestamp() {
        assert_eq!(
            timestamp_from_name("SM-F936B_20250424134351"),
            Some("2025-04-24T13:43:51".to_string())
        );
    }
}
