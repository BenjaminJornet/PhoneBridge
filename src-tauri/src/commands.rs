use crate::adapters::{self, BackupSource, CategoryMetric};
use crate::adb;
use crate::db::{self, IndexSummary, IndexedFile};
use crate::library::{
    self, BackupCoverage, ConsolidationConfig, ConsolidationPlan, ConsolidationResult,
};
use crate::smartswitch::{self, SmartSwitchArchiveInventory, SmartSwitchItemMetric};
use crate::sync::{self, SmartSwitchCategory, SmartSwitchSyncConfig, SmartSwitchSyncResult};
use std::path::PathBuf;

#[tauri::command]
pub fn scan_backup_sources() -> Result<Vec<BackupSource>, String> {
    adapters::scan_default_sources().map_err(|err| err.to_string())
}

#[tauri::command]
pub fn detect_adb_devices() -> Result<Vec<BackupSource>, String> {
    adb::detect_devices().map_err(|err| err.to_string())
}

#[tauri::command]
pub fn get_category_metrics() -> Result<Vec<CategoryMetric>, String> {
    let indexed = db::get_indexed_category_metrics().map_err(|err| err.to_string())?;
    if indexed.iter().any(|metric| metric.count > 0) {
        return Ok(indexed);
    }

    adapters::get_local_category_metrics().map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn index_multimedia(source_path: String) -> Result<IndexSummary, String> {
    tauri::async_runtime::spawn_blocking(move || db::index_folder(source_path))
        .await
        .map_err(|err| err.to_string())?
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn list_indexed_files(
    category: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<IndexedFile>, String> {
    db::list_default_indexed_files(category, limit).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn get_smartswitch_item_metrics() -> Result<Vec<SmartSwitchItemMetric>, String> {
    smartswitch::read_default_item_metrics().map_err(|err| err.to_string())
}

#[tauri::command]
pub fn get_smartswitch_archive_inventory() -> Result<Vec<SmartSwitchArchiveInventory>, String> {
    smartswitch::read_default_archive_inventory().map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn scan_smartswitch_categories(
    source_path: String,
) -> Result<Vec<SmartSwitchCategory>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        sync::scan_smartswitch_categories(&PathBuf::from(source_path))
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn run_smartswitch_sync(
    config: SmartSwitchSyncConfig,
) -> Result<SmartSwitchSyncResult, String> {
    tauri::async_runtime::spawn_blocking(move || sync::execute_smartswitch_sync(config))
        .await
        .map_err(|err| err.to_string())?
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn plan_consolidation(config: ConsolidationConfig) -> Result<ConsolidationPlan, String> {
    tauri::async_runtime::spawn_blocking(move || library::plan_consolidation(config))
        .await
        .map_err(|err| err.to_string())?
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn run_consolidation(
    window: tauri::Window,
    config: ConsolidationConfig,
) -> Result<ConsolidationResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        library::execute_consolidation_with_progress(config, window)
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn list_backup_coverage() -> Result<Vec<BackupCoverage>, String> {
    library::list_backup_coverage().map_err(|err| err.to_string())
}
