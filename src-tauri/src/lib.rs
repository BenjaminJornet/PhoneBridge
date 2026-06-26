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

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::detect_adb_devices,
            commands::diagnose_adb,
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
            commands::open_file,
            commands::reveal_in_finder,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run PhoneBridge");
}
