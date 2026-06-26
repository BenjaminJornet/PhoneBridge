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
    Arc, Mutex,
};

use tauri::Emitter;

pub struct PullCancelToken(pub Arc<AtomicBool>);

/// Ref-count of currently-open modals that want Escape captured at the native layer. The
/// global Escape shortcut is registered while this is > 0 and unregistered when it drops to 0,
/// so Escape is only intercepted while a modal is on screen. See `commands::enable_escape_capture`.
pub struct EscapeCapture(pub Mutex<u32>);

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
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    // Only the modal-gated Escape shortcut is ever registered; relay its press
                    // to the webview, which closes the open modal (works around macOS WKWebView
                    // swallowing the Escape keydown before JS sees it).
                    if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        let _ = app.emit("modal-escape", ());
                    }
                })
                .build(),
        )
        .manage(PullCancelToken::new())
        .manage(EscapeCapture(Mutex::new(0)))
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
            commands::enable_escape_capture,
            commands::disable_escape_capture,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run PhoneBridge");
}
