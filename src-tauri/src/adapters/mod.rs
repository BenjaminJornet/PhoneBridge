use serde::Serialize;
use thiserror::Error;

pub mod adb_generic;
pub mod smartswitch;

#[derive(Debug, Error)]
pub enum AdapterError {
    #[error("filesystem error: {0}")]
    Filesystem(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
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

pub trait BackupAdapter {
    fn scan(&self) -> Result<Vec<BackupSource>, AdapterError>;
}

pub fn scan_default_sources() -> Result<Vec<BackupSource>, AdapterError> {
    let mut sources = Vec::new();
    sources.extend(smartswitch::SmartSwitchAdapter.scan()?);
    Ok(sources)
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
