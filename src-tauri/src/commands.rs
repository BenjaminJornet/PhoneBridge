use crate::adapters::{self, BackupSource, CategoryMetric};
use crate::adb;
use crate::db::{self, IndexSummary, IndexedFile};
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
pub fn index_multimedia() -> Result<IndexSummary, String> {
    db::index_default_multimedia().map_err(|err| err.to_string())
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
pub fn scan_smartswitch_categories(
    source_path: String,
) -> Result<Vec<SmartSwitchCategory>, String> {
    sync::scan_smartswitch_categories(&PathBuf::from(source_path)).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn run_smartswitch_sync(
    config: SmartSwitchSyncConfig,
) -> Result<SmartSwitchSyncResult, String> {
    sync::execute_smartswitch_sync(config).map_err(|err| err.to_string())
}
