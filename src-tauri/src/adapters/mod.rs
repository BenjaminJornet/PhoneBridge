use serde::Serialize;
use std::env;
use std::path::{Path, PathBuf};
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
    sources.extend(smartswitch::SmartSwitchAdapter::default().scan()?);
    Ok(sources)
}

pub fn get_local_category_metrics() -> Result<Vec<CategoryMetric>, AdapterError> {
    let root = samsung_root().join("Multimedia");
    let categories = ["Photo", "Video", "Music", "Documents"];
    let mut metrics = Vec::new();

    for category in categories {
        let path = root.join(category);
        let (count, bytes) = summarize_directory(&path)?;
        metrics.push(CategoryMetric {
            category: category.to_lowercase(),
            count,
            bytes,
        });
    }

    Ok(metrics)
}

pub fn samsung_root() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Samsung")
}

fn summarize_directory(path: &Path) -> Result<(u64, u64), AdapterError> {
    if !path.exists() {
        return Ok((0, 0));
    }

    let mut count = 0;
    let mut bytes = 0;

    for entry in walkdir::WalkDir::new(path) {
        let entry = entry?;
        if entry.file_type().is_file() {
            count += 1;
            bytes += entry.metadata()?.len();
        }
    }

    Ok((count, bytes))
}
