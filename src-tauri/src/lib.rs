mod adapters;
mod adb;
mod commands;
mod db;
mod library;
mod media;
mod path_utils;
mod privacy;
mod smartswitch;
mod sync;
mod whatsapp;

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

pub struct PullCancelToken(pub Arc<AtomicBool>);

impl PullCancelToken {
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
    pub fn reset(&self) {
        self.0.store(false, Ordering::Relaxed);
    }
    pub fn arc(&self) -> Arc<AtomicBool> {
        self.0.clone()
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(PullCancelToken::new())
        .invoke_handler(tauri::generate_handler![
            commands::detect_adb_devices,
            commands::diagnose_adb,
            commands::find_duplicate_files,
            commands::find_similar_photos,
            commands::move_files_to_trash,
            commands::get_adapter_registry,
            commands::get_category_metrics,
            commands::get_smartswitch_archive_inventory,
            commands::get_smartswitch_item_metrics,
            commands::get_structured_records,
            commands::index_multimedia,
            commands::list_backup_coverage,
            commands::list_indexed_files,
            commands::plan_consolidation,
            commands::preview_device_media,
            commands::pull_from_device,
            commands::pull_whatsapp_database,
            commands::decrypt_whatsapp_database,
            commands::run_consolidation,
            commands::run_smartswitch_sync,
            commands::scan_backup_sources,
            commands::scan_smartswitch_categories,
            commands::cancel_pull_from_device,
            commands::open_file,
            commands::reveal_in_finder,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run PhoneBridge");
}
