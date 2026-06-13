mod adapters;
mod adb;
mod commands;
mod db;
mod media;
mod smartswitch;
mod sync;

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::detect_adb_devices,
            commands::get_category_metrics,
            commands::get_smartswitch_archive_inventory,
            commands::get_smartswitch_item_metrics,
            commands::index_multimedia,
            commands::list_indexed_files,
            commands::run_smartswitch_sync,
            commands::scan_backup_sources,
            commands::scan_smartswitch_categories,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run PhoneBridge");
}
