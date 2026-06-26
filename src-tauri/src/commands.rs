use crate::adapters::{self, AdapterDefinition, BackupSource, CategoryMetric};
use crate::adb::{self, AdbDiagnostic, AdbMediaFolderPreview, AdbPullResult, WhatsAppPullResult};
use crate::db::{self, IndexSummary, IndexedFile};
use crate::library::{
    self, BackupCoverage, ConsolidationConfig, ConsolidationPlan, ConsolidationResult,
};
use crate::smartswitch::{
    self, SmartSwitchArchiveInventory, SmartSwitchItemMetric, StructuredRecord,
};
use crate::sync::{self, SmartSwitchCategory, SmartSwitchSyncConfig, SmartSwitchSyncResult};
use crate::whatsapp::{self, WhatsAppDecryptConfig, WhatsAppDecryptResult};
use crate::PullCancelToken;
use std::path::PathBuf;

#[tauri::command]
pub fn scan_backup_sources() -> Result<Vec<BackupSource>, String> {
    adapters::scan_default_sources().map_err(|err| err.to_string())
}

#[tauri::command]
pub fn get_adapter_registry() -> Vec<AdapterDefinition> {
    adapters::adapter_registry()
}

#[tauri::command]
pub fn detect_adb_devices() -> Result<Vec<BackupSource>, String> {
    adb::detect_devices().map_err(|err| err.to_string())
}

#[tauri::command]
pub fn diagnose_adb() -> AdbDiagnostic {
    adb::diagnose_adb()
}

#[tauri::command]
pub async fn preview_device_media(source_id: String) -> Result<Vec<AdbMediaFolderPreview>, String> {
    tauri::async_runtime::spawn_blocking(move || adb::preview_device_media_by_source_id(&source_id))
        .await
        .map_err(|err| err.to_string())?
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn pull_from_device(
    window: tauri::Window,
    source_id: String,
    destination_path: String,
    selected_keys: Option<Vec<String>>,
    cancel_token: tauri::State<'_, PullCancelToken>,
) -> Result<AdbPullResult, String> {
    cancel_token.reset();
    let token = cancel_token.arc();
    tauri::async_runtime::spawn_blocking(move || {
        adb::pull_device_media_by_source_id(
            &source_id,
            &crate::path_utils::expand_home(&destination_path),
            selected_keys,
            Some(window),
            token,
        )
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn cancel_pull_from_device(cancel_token: tauri::State<'_, PullCancelToken>) {
    cancel_token.cancel();
}

#[tauri::command]
pub async fn pull_whatsapp_database(
    source_id: String,
    destination_dir: String,
) -> Result<WhatsAppPullResult, String> {
    let dir = crate::path_utils::expand_home(&destination_dir);
    tauri::async_runtime::spawn_blocking(move || {
        adb::pull_whatsapp_database_by_source_id(&source_id, &dir)
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(|err| err.to_string())
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
    offset: Option<u32>,
) -> Result<Vec<IndexedFile>, String> {
    db::list_default_indexed_files(category, limit, offset).map_err(|err| err.to_string())
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
pub fn get_structured_records() -> Result<Vec<StructuredRecord>, String> {
    smartswitch::read_default_structured_records().map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn decrypt_whatsapp_database(
    config: WhatsAppDecryptConfig,
) -> Result<WhatsAppDecryptResult, String> {
    tauri::async_runtime::spawn_blocking(move || whatsapp::decrypt_whatsapp_database(config))
        .await
        .map_err(|err| err.to_string())?
        .map_err(|err| err.to_string())
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
    window: tauri::Window,
    config: SmartSwitchSyncConfig,
) -> Result<SmartSwitchSyncResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        sync::execute_smartswitch_sync_with_window(config, window)
    })
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
pub async fn find_duplicate_files(
    window: tauri::Window,
    category: Option<String>,
) -> Result<db::DuplicateScanResult, String> {
    use tauri::Emitter;
    tauri::async_runtime::spawn_blocking(move || {
        db::find_default_duplicate_files(category, |done, total| {
            let _ = window.emit(
                "duplicate-scan-progress",
                serde_json::json!({ "done": done, "total": total }),
            );
        })
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn find_similar_photos(
    window: tauri::Window,
) -> Result<db::DuplicateScanResult, String> {
    use tauri::Emitter;
    tauri::async_runtime::spawn_blocking(move || {
        db::find_default_similar_photos(|done, total| {
            let _ = window.emit(
                "similar-scan-progress",
                serde_json::json!({ "done": done, "total": total }),
            );
        })
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn move_files_to_trash(paths: Vec<String>) -> Result<db::TrashResult, String> {
    tauri::async_runtime::spawn_blocking(move || db::move_files_to_trash(paths))
        .await
        .map_err(|err| err.to_string())?
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn list_backup_coverage() -> Result<Vec<BackupCoverage>, String> {
    library::list_backup_coverage().map_err(|err| err.to_string())
}

#[tauri::command]
pub fn open_file(path: String) -> Result<(), String> {
    std::process::Command::new("open")
        .arg(&path)
        .spawn()
        .map(|_| ())
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn reveal_in_finder(path: String) -> Result<(), String> {
    std::process::Command::new("open")
        .arg("-R")
        .arg(&path)
        .spawn()
        .map(|_| ())
        .map_err(|err| err.to_string())
}
