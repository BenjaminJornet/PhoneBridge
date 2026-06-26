use serde::Serialize;

mod commands;
mod discovery;
mod media;
mod whatsapp_pull;

pub use discovery::{detect_devices, diagnose_adb};
pub use media::{preview_device_media_by_source_id, pull_device_media_by_source_id};
// `pull_device_media` was crate-reachable as `crate::adb::pull_device_media` before the split;
// re-export to preserve that path even though only the `*_by_source_id` wrapper is used today.
#[allow(unused_imports)]
pub use media::pull_device_media;
pub use whatsapp_pull::{pull_whatsapp_database_by_source_id, WhatsAppPullResult};

pub(super) const ADB_MEDIA_PATHS: [(&str, &str); 6] = [
    ("DCIM", "/sdcard/DCIM/"),
    ("Pictures", "/sdcard/Pictures/"),
    ("Download", "/sdcard/Download/"),
    ("Movies", "/sdcard/Movies/"),
    ("Music", "/sdcard/Music/"),
    (
        "WhatsApp Media",
        "/storage/emulated/0/Android/media/com.whatsapp/WhatsApp/Media/",
    ),
];

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdbPullProgress {
    pub pulled_paths: u64,
    pub skipped_paths: u64,
    pub pulled_files: u64,
    pub skipped_files: u64,
    pub permission_denied_files: u64,
    pub total_files: u64,
    pub current_path: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdbMediaFolderPreview {
    pub key: String,
    pub label: String,
    pub remote_path: String,
    pub file_count: u64,
    pub total_bytes: u64,
    pub available: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdbPullResult {
    pub source_path: String,
    pub pulled_paths: u64,
    pub skipped_paths: u64,
    pub pulled_files: u64,
    pub skipped_files: u64,
    pub permission_denied_files: u64,
    pub total_files: u64,
    pub errors: Vec<String>,
    pub cancelled: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdbDiagnosticDevice {
    pub source_id: String,
    pub label: String,
    pub status: String,
    pub model: Option<String>,
    pub manufacturer: Option<String>,
    pub android_version: Option<String>,
    pub redacted_id: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdbDiagnostic {
    pub adb_found: bool,
    pub adb_path: Option<String>,
    pub devices: Vec<AdbDiagnosticDevice>,
    pub message: String,
    pub next_action: String,
}
