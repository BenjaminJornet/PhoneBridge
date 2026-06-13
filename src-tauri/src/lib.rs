mod adapters;
mod adb;
mod commands;
mod db;
mod library;
mod media;
mod privacy;
mod smartswitch;
mod sync;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::detect_adb_devices,
            commands::get_adapter_registry,
            commands::get_category_metrics,
            commands::get_smartswitch_archive_inventory,
            commands::get_smartswitch_item_metrics,
            commands::get_structured_records,
            commands::index_multimedia,
            commands::list_backup_coverage,
            commands::list_indexed_files,
            commands::plan_consolidation,
            commands::pull_from_device,
            commands::run_consolidation,
            commands::run_smartswitch_sync,
            commands::scan_backup_sources,
            commands::scan_smartswitch_categories,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run PhoneBridge");
}
