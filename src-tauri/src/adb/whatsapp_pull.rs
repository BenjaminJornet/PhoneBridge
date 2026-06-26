use crate::adapters::AdapterError;
use serde::Serialize;
use std::fs;
use std::path::Path;

use super::commands::{adb_pull, adb_shell_capture};
use super::discovery::resolve_serial_for_source_id;

/// Candidate on-device locations for the WhatsApp message database, newest storage
/// layout first. Only the encrypted database lives in user-readable storage; the
/// decryption key stays in `/data/data/com.whatsapp/` and cannot be pulled without root.
const WHATSAPP_DB_DIRS: [&str; 3] = [
    "/storage/emulated/0/Android/media/com.whatsapp/WhatsApp/Databases",
    "/sdcard/WhatsApp/Databases",
    "/storage/emulated/0/WhatsApp/Databases",
];

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WhatsAppPullResult {
    pub local_path: String,
    pub remote_path: String,
    pub format: String,
}

pub fn pull_whatsapp_database_by_source_id(
    source_id: &str,
    destination_dir: &Path,
) -> Result<WhatsAppPullResult, AdapterError> {
    let serial = resolve_serial_for_source_id(source_id)?;
    for dir in WHATSAPP_DB_DIRS {
        let listing = adb_shell_capture(&serial, &format!("ls -1 {dir}"));
        let Some(file_name) = pick_whatsapp_database(&listing) else {
            continue;
        };
        let remote_path = format!("{dir}/{file_name}");
        fs::create_dir_all(destination_dir)?;
        let local_path = destination_dir.join(&file_name);
        adb_pull(&serial, &remote_path, &local_path)?;
        let format = if file_name.ends_with(".crypt15") {
            "crypt15"
        } else if file_name.ends_with(".crypt14") {
            "crypt14"
        } else {
            "unknown"
        };
        return Ok(WhatsAppPullResult {
            local_path: local_path.to_string_lossy().into_owned(),
            remote_path,
            format: format.to_string(),
        });
    }
    Err(AdapterError::CommandFailed(
        "No WhatsApp database (msgstore.db.crypt15) found on the device. Make sure WhatsApp is installed and has created a local backup.".to_string(),
    ))
}

/// Pick the most relevant encrypted database from a directory listing: the current
/// (undated) `msgstore.db.crypt15/14` if present, otherwise the newest dated backup
/// (the `msgstore-YYYY-MM-DD` prefix sorts chronologically).
fn pick_whatsapp_database(listing: &str) -> Option<String> {
    let mut candidates: Vec<&str> = listing
        .lines()
        .map(|line| line.trim())
        .filter(|line| line.ends_with(".crypt15") || line.ends_with(".crypt14"))
        .collect();
    if candidates.is_empty() {
        return None;
    }
    for preferred in ["msgstore.db.crypt15", "msgstore.db.crypt14"] {
        if candidates.contains(&preferred) {
            return Some(preferred.to_string());
        }
    }
    candidates.sort_unstable();
    candidates.last().map(|name| (*name).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_current_whatsapp_database_over_dated_backups() {
        let listing = "msgstore-2026-06-20.1.db.crypt15\nmsgstore.db.crypt15\nmsgstore-2026-06-19.1.db.crypt15\nchatsettings.db\n";
        assert_eq!(
            pick_whatsapp_database(listing),
            Some("msgstore.db.crypt15".to_string())
        );
    }

    #[test]
    fn falls_back_to_newest_dated_whatsapp_backup() {
        let listing = "msgstore-2026-06-19.1.db.crypt15\nmsgstore-2026-06-21.1.db.crypt15\nmsgstore-2026-06-20.1.db.crypt15\n";
        assert_eq!(
            pick_whatsapp_database(listing),
            Some("msgstore-2026-06-21.1.db.crypt15".to_string())
        );
    }

    #[test]
    fn returns_none_when_no_encrypted_whatsapp_database_present() {
        assert_eq!(pick_whatsapp_database("chatsettings.db\nstickers.db\n"), None);
    }
}
