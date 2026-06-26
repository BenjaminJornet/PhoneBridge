use crate::adapters::AdapterError;
use serde::Deserialize;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use zip::ZipArchive;

use super::{SmartSwitchArchiveInventory, SmartSwitchItemMetric};

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

pub(super) fn read_folder_inventory(
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

pub(super) fn read_item_metrics(
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

pub(super) fn read_archive_inventory(
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

pub(super) fn looks_like_text_xml<R: Read>(reader: &mut R) -> bool {
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
