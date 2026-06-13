use serde::Serialize;
use thiserror::Error;

pub mod adb_generic;
pub mod folder;
pub mod smartswitch;
pub mod takeout;

#[derive(Debug, Error)]
pub enum AdapterError {
    #[error("command unavailable: {0}")]
    CommandUnavailable(String),
    #[error("command failed: {0}")]
    CommandFailed(String),
    #[error("filesystem error: {0}")]
    Filesystem(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("walkdir error: {0}")]
    Walkdir(#[from] walkdir::Error),
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSummary {
    pub id: String,
    pub model: String,
    pub manufacturer: String,
    pub android_version: Option<String>,
    pub connection: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupSource {
    pub id: String,
    pub adapter: String,
    pub label: String,
    pub path: Option<String>,
    pub device: Option<DeviceSummary>,
    pub created_at: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryMetric {
    pub category: String,
    pub count: u64,
    pub bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterDefinition {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
}

pub trait BackupAdapter {
    fn definition(&self) -> AdapterDefinition;
    fn scan(&self) -> Result<Vec<BackupSource>, AdapterError>;
}

pub fn adapter_registry() -> Vec<AdapterDefinition> {
    registered_adapters()
        .into_iter()
        .map(|adapter| adapter.definition())
        .collect()
}

pub fn scan_default_sources() -> Result<Vec<BackupSource>, AdapterError> {
    let mut sources = Vec::new();
    for adapter in registered_adapters() {
        sources.extend(adapter.scan()?);
    }
    Ok(sources)
}

fn registered_adapters() -> Vec<Box<dyn BackupAdapter>> {
    vec![
        Box::new(adb_generic::AdbGenericAdapter),
        Box::new(folder::FolderAdapter),
        Box::new(smartswitch::SmartSwitchAdapter),
        Box::new(takeout::TakeoutAdapter),
    ]
}

pub fn get_local_category_metrics() -> Result<Vec<CategoryMetric>, AdapterError> {
    let categories = ["Photo", "Video", "Music", "Documents"];
    Ok(categories
        .into_iter()
        .map(|category| CategoryMetric {
            category: category.to_lowercase(),
            count: 0,
            bytes: 0,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_exposes_core_adapters() {
        let ids: Vec<_> = adapter_registry().into_iter().map(|item| item.id).collect();
        assert!(ids.contains(&"generic-folder"));
        assert!(ids.contains(&"adb-generic"));
        assert!(ids.contains(&"samsung-smartswitch"));
        assert!(ids.contains(&"google-takeout"));
    }
}
