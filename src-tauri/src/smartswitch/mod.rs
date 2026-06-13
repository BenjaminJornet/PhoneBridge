use crate::adapters::smartswitch::SmartSwitchAdapter;
use crate::adapters::{AdapterError, BackupAdapter};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use zip::ZipArchive;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartSwitchItemMetric {
    pub backup_id: String,
    pub backup_label: String,
    pub item_type: String,
    pub view_count: u64,
    pub content_count: u64,
    pub size_bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartSwitchArchiveInventory {
    pub backup_id: String,
    pub backup_label: String,
    pub item_type: String,
    pub archive_path: String,
    pub entry_count: u64,
    pub encrypted_entries: u64,
    pub image_entries: u64,
    pub blob_entries: u64,
    pub parse_status: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ReqItemsInfo {
    list_items: Vec<ReqItem>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ReqItem {
    view_count: Option<u64>,
    content_count: Option<u64>,
    #[serde(rename = "Type")]
    item_type: String,
    #[serde(rename = "Size")]
    size_bytes: Option<u64>,
}

pub fn read_default_item_metrics() -> Result<Vec<SmartSwitchItemMetric>, AdapterError> {
    let sources = SmartSwitchAdapter.scan()?;
    let mut metrics = Vec::new();

    for source in sources {
        let Some(path) = source.path.as_deref() else {
            continue;
        };
        metrics.extend(read_item_metrics(
            Path::new(path),
            &source.id,
            &source.label,
        )?);
    }

    Ok(metrics)
}

pub fn read_default_archive_inventory() -> Result<Vec<SmartSwitchArchiveInventory>, AdapterError> {
    let sources = SmartSwitchAdapter.scan()?;
    let mut inventories = Vec::new();

    for source in sources {
        let Some(path) = source.path.as_deref() else {
            continue;
        };
        let backup_path = Path::new(path);
        inventories.extend(read_archive_inventory(
            backup_path,
            &source.id,
            &source.label,
            "CONTACT",
            backup_path.join("CONTACT").join("Contact.SPBM"),
        )?);
        inventories.extend(read_archive_inventory(
            backup_path,
            &source.id,
            &source.label,
            "CALLLOG",
            backup_path.join("CALLLOG").join("CALLLOG.zip"),
        )?);
        inventories.extend(read_folder_inventory(
            &source.id,
            &source.label,
            "CALENDER",
            backup_path.join("CALENDER"),
        )?);
    }

    Ok(inventories)
}

fn read_folder_inventory(
    backup_id: &str,
    backup_label: &str,
    item_type: &str,
    folder_path: PathBuf,
) -> Result<Vec<SmartSwitchArchiveInventory>, AdapterError> {
    if !folder_path.exists() {
        return Ok(Vec::new());
    }
    let mut entry_count = 0;
    for entry in walkdir::WalkDir::new(&folder_path) {
        let entry = entry?;
        if entry.file_type().is_file() {
            entry_count += 1;
        }
    }
    Ok(vec![SmartSwitchArchiveInventory {
        backup_id: backup_id.to_string(),
        backup_label: backup_label.to_string(),
        item_type: item_type.to_string(),
        archive_path: folder_path.to_string_lossy().into_owned(),
        entry_count,
        encrypted_entries: 0,
        image_entries: 0,
        blob_entries: 0,
        parse_status: "inventory_only".to_string(),
    }])
}

fn read_item_metrics(
    backup_path: &Path,
    backup_id: &str,
    backup_label: &str,
) -> Result<Vec<SmartSwitchItemMetric>, AdapterError> {
    let req_items_path = backup_path.join("ReqItemsInfo.json");
    if !req_items_path.exists() {
        return Ok(Vec::new());
    }

    let raw = fs::read_to_string(req_items_path)?;
    let parsed: ReqItemsInfo = serde_json::from_str(&raw)?;

    Ok(parsed
        .list_items
        .into_iter()
        .map(|item| SmartSwitchItemMetric {
            backup_id: backup_id.to_string(),
            backup_label: backup_label.to_string(),
            item_type: item.item_type,
            view_count: item.view_count.unwrap_or(0),
            content_count: item.content_count.unwrap_or(0),
            size_bytes: item.size_bytes.unwrap_or(0),
        })
        .collect())
}

fn read_archive_inventory(
    _backup_path: &Path,
    backup_id: &str,
    backup_label: &str,
    item_type: &str,
    archive_path: PathBuf,
) -> Result<Vec<SmartSwitchArchiveInventory>, AdapterError> {
    if !archive_path.exists() {
        return Ok(Vec::new());
    }

    let archive = File::open(&archive_path)?;
    let mut zip = ZipArchive::new(archive).map_err(|err| {
        AdapterError::Filesystem(std::io::Error::new(std::io::ErrorKind::InvalidData, err))
    })?;

    let mut encrypted_entries = 0;
    let mut image_entries = 0;
    let mut blob_entries = 0;
    let mut parse_status = "inventory_only".to_string();

    for index in 0..zip.len() {
        let mut file = zip.by_index(index).map_err(|err| {
            AdapterError::Filesystem(std::io::Error::new(std::io::ErrorKind::InvalidData, err))
        })?;
        let name = file.name().to_string();
        if name.ends_with(".enc") {
            encrypted_entries += 1;
        }
        if name.contains("/image/") || name.ends_with(".jpg") || name.ends_with(".png") {
            image_entries += 1;
        }
        if name.contains("/blob/") {
            blob_entries += 1;
        }

        if item_type == "CALLLOG" && name.ends_with("call_log.exml") {
            parse_status = if looks_like_text_xml(&mut file) {
                "xml_readable".to_string()
            } else {
                "binary_or_encrypted_exml".to_string()
            };
        }
    }

    if item_type == "CONTACT" && encrypted_entries > 0 {
        parse_status = "encrypted_contact_payload".to_string();
    }

    Ok(vec![SmartSwitchArchiveInventory {
        backup_id: backup_id.to_string(),
        backup_label: backup_label.to_string(),
        item_type: item_type.to_string(),
        archive_path: archive_path.to_string_lossy().into_owned(),
        entry_count: zip.len() as u64,
        encrypted_entries,
        image_entries,
        blob_entries,
        parse_status,
    }])
}

fn looks_like_text_xml<R: Read>(reader: &mut R) -> bool {
    let mut buffer = [0_u8; 128];
    let Ok(size) = reader.read(&mut buffer) else {
        return false;
    };
    let sample = &buffer[..size];
    sample.starts_with(b"<?xml")
        || sample
            .iter()
            .all(|byte| byte.is_ascii() || byte.is_ascii_whitespace())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn parses_req_items_info_metrics() {
        let temp = tempdir().unwrap();
        fs::write(
            temp.path().join("ReqItemsInfo.json"),
            r#"{"ListItems":[{"ViewCount":12,"ContentCount":12,"Type":"CONTACT","Size":2048},{"ViewCount":34,"ContentCount":34,"Type":"MESSAGE","Size":4096}]}"#,
        )
        .unwrap();

        let metrics = read_item_metrics(temp.path(), "backup-1", "Galaxy Backup").unwrap();
        assert_eq!(metrics.len(), 2);
        assert_eq!(metrics[0].item_type, "CONTACT");
        assert_eq!(metrics[0].content_count, 12);
        assert_eq!(metrics[1].item_type, "MESSAGE");
        assert_eq!(metrics[1].size_bytes, 4096);
    }

    #[test]
    fn detects_binary_call_log_payload() {
        assert!(!looks_like_text_xml(&mut &[0, 159, 146, 150][..]));
        assert!(looks_like_text_xml(&mut &b"<?xml version=\"1.0\"?>"[..]));
    }

    #[test]
    fn inventories_calendar_folder() {
        let temp = tempdir().unwrap();
        let calendar = temp.path().join("CALENDER");
        fs::create_dir_all(&calendar).unwrap();
        fs::write(calendar.join("event.vcs"), b"BEGIN:VCALENDAR").unwrap();

        let inventory = read_folder_inventory("backup", "Backup", "CALENDER", calendar).unwrap();
        assert_eq!(inventory[0].entry_count, 1);
        assert_eq!(inventory[0].parse_status, "inventory_only");
    }
}
