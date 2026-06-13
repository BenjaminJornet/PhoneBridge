use crate::adapters::smartswitch::SmartSwitchAdapter;
use crate::adapters::{AdapterError, BackupAdapter};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use zip::ZipArchive;

const BROWSER_STATUS: &str = "inventory_only_proprietary_payload";
const WHATSAPP_MESSAGE_STATUS: &str = "encrypted_whatsapp_message_database";

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

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuredRecord {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub source_path: String,
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

pub fn read_default_structured_records() -> Result<Vec<StructuredRecord>, AdapterError> {
    let sources = SmartSwitchAdapter.scan()?;
    let mut records = Vec::new();

    for source in sources {
        let Some(path) = source.path.as_deref() else {
            continue;
        };
        records.extend(read_structured_records(Path::new(path), &source.id)?);
    }

    Ok(records)
}

fn read_structured_records(
    backup_path: &Path,
    backup_id: &str,
) -> Result<Vec<StructuredRecord>, AdapterError> {
    let mut records = Vec::new();
    records.extend(read_calendar_records(backup_path, backup_id)?);
    records.extend(read_note_records(backup_path, backup_id)?);
    records.extend(read_browser_records(backup_path, backup_id)?);
    records.extend(read_whatsapp_message_records(backup_path, backup_id)?);
    records.extend(read_status_records(backup_path, backup_id)?);
    Ok(records)
}

fn read_browser_records(
    backup_path: &Path,
    backup_id: &str,
) -> Result<Vec<StructuredRecord>, AdapterError> {
    let folder = backup_path.join("SBROWSER");
    if !folder.exists() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    for entry in walkdir::WalkDir::new(&folder) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();

        match extension.as_str() {
            "json" => records.extend(parse_browser_json(backup_id, path)?),
            "html" | "htm" => records.extend(parse_browser_html(backup_id, path)?),
            _ => {}
        }
    }

    if records.is_empty() {
        records.push(status_record(backup_id, "browser", &folder, BROWSER_STATUS));
    }

    Ok(records)
}

fn parse_browser_json(backup_id: &str, path: &Path) -> Result<Vec<StructuredRecord>, AdapterError> {
    let raw = fs::read_to_string(path)?;
    let parsed: serde_json::Value = serde_json::from_str(&raw)?;
    let mut records = Vec::new();
    collect_browser_json_records(backup_id, path, &parsed, &mut records);
    Ok(records)
}

fn collect_browser_json_records(
    backup_id: &str,
    path: &Path,
    value: &serde_json::Value,
    records: &mut Vec<StructuredRecord>,
) {
    match value {
        serde_json::Value::Array(items) => {
            for item in items {
                collect_browser_json_records(backup_id, path, item, records);
            }
        }
        serde_json::Value::Object(map) => {
            let title = first_string(map, &["title", "name"]);
            let url = first_string(map, &["url", "uri"]);
            if title.is_some() || url.is_some() {
                let fallback = url.clone().unwrap_or_else(|| "Browser record".to_string());
                records.push(StructuredRecord {
                    id: format!(
                        "{backup_id}:browser:{}:{}",
                        path.to_string_lossy(),
                        records.len()
                    ),
                    kind: "browser".to_string(),
                    title: title.unwrap_or(fallback),
                    subtitle: url,
                    source_path: path.to_string_lossy().into_owned(),
                    parse_status: "parsed_browser_json".to_string(),
                });
            }

            for child in map.values() {
                collect_browser_json_records(backup_id, path, child, records);
            }
        }
        _ => {}
    }
}

fn first_string(map: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        map.get(*key)
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
    })
}

fn parse_browser_html(backup_id: &str, path: &Path) -> Result<Vec<StructuredRecord>, AdapterError> {
    let raw = fs::read_to_string(path)?;
    let mut records = Vec::new();
    let mut rest = raw.as_str();
    while let Some(anchor_start) = rest.find("<a") {
        rest = &rest[anchor_start..];
        let Some(tag_end) = rest.find('>') else {
            break;
        };
        let tag = &rest[..tag_end];
        let Some(close) = rest[tag_end + 1..].find("</a>") else {
            break;
        };
        let title = strip_html_text(&rest[tag_end + 1..tag_end + 1 + close]);
        let url = extract_href(tag);
        if !title.is_empty() || url.is_some() {
            let fallback = url.clone().unwrap_or_else(|| "Browser record".to_string());
            records.push(StructuredRecord {
                id: format!(
                    "{backup_id}:browser:{}:{}",
                    path.to_string_lossy(),
                    records.len()
                ),
                kind: "browser".to_string(),
                title: if title.is_empty() { fallback } else { title },
                subtitle: url,
                source_path: path.to_string_lossy().into_owned(),
                parse_status: "parsed_browser_html".to_string(),
            });
        }
        rest = &rest[tag_end + 1 + close + 4..];
    }
    Ok(records)
}

fn extract_href(tag: &str) -> Option<String> {
    for marker in ["href=\"", "HREF=\""] {
        if let Some(start) = tag.find(marker) {
            let value_start = start + marker.len();
            return tag[value_start..]
                .find('"')
                .map(|end| tag[value_start..value_start + end].to_string());
        }
    }
    None
}

fn strip_html_text(value: &str) -> String {
    let mut output = String::new();
    let mut inside_tag = false;
    for character in value.chars() {
        match character {
            '<' => inside_tag = true,
            '>' => inside_tag = false,
            _ if !inside_tag => output.push(character),
            _ => {}
        }
    }
    output.trim().to_string()
}

fn read_whatsapp_message_records(
    backup_path: &Path,
    backup_id: &str,
) -> Result<Vec<StructuredRecord>, AdapterError> {
    let mut records = Vec::new();
    for folder_name in ["MESSAGE", "WhatsApp"] {
        let folder = backup_path.join(folder_name);
        if !folder.exists() {
            continue;
        }

        for entry in walkdir::WalkDir::new(&folder) {
            let entry = entry?;
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let file_name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            if file_name.starts_with("msgstore")
                || file_name.starts_with("wa.db")
                || file_name.ends_with(".crypt12")
                || file_name.ends_with(".crypt14")
                || file_name.ends_with(".crypt15")
            {
                records.push(status_record(
                    backup_id,
                    "whatsapp_message",
                    path,
                    WHATSAPP_MESSAGE_STATUS,
                ));
            }
        }
    }
    Ok(records)
}

fn read_note_records(
    backup_path: &Path,
    backup_id: &str,
) -> Result<Vec<StructuredRecord>, AdapterError> {
    let folder = backup_path.join("SAMSUNGNOTE");
    if !folder.exists() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    for entry in walkdir::WalkDir::new(&folder) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if matches!(extension.as_str(), "txt" | "md" | "html") {
            let raw = fs::read_to_string(path)?;
            let title = raw
                .lines()
                .find(|line| !line.trim().is_empty())
                .map(|line| line.trim().chars().take(80).collect::<String>())
                .unwrap_or_else(|| "Samsung note".to_string());
            records.push(StructuredRecord {
                id: format!("{backup_id}:note:{}", path.to_string_lossy()),
                kind: "note".to_string(),
                title,
                subtitle: Some(format!("{} bytes", raw.len())),
                source_path: path.to_string_lossy().into_owned(),
                parse_status: "parsed_text_note".to_string(),
            });
        } else {
            records.push(status_record(
                backup_id,
                "note",
                path,
                "proprietary_note_payload",
            ));
        }
    }

    Ok(records)
}

fn read_calendar_records(
    backup_path: &Path,
    backup_id: &str,
) -> Result<Vec<StructuredRecord>, AdapterError> {
    let folder = backup_path.join("CALENDER");
    if !folder.exists() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    for entry in walkdir::WalkDir::new(&folder) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if !matches!(extension.to_ascii_lowercase().as_str(), "ics" | "vcs") {
            records.push(status_record(
                backup_id,
                "calendar",
                path,
                "unsupported_calendar_file",
            ));
            continue;
        }

        let file = File::open(path)?;
        let mut title = None;
        let mut date = None;
        for line in BufReader::new(file).lines() {
            let line = line?;
            if let Some(value) = line.strip_prefix("SUMMARY:") {
                title = Some(value.to_string());
            }
            if let Some(value) = line.strip_prefix("DTSTART") {
                date = value.split_once(':').map(|(_, right)| right.to_string());
            }
        }
        records.push(StructuredRecord {
            id: format!("{backup_id}:{}", path.to_string_lossy()),
            kind: "calendar".to_string(),
            title: title.unwrap_or_else(|| "Calendar event".to_string()),
            subtitle: date,
            source_path: path.to_string_lossy().into_owned(),
            parse_status: "parsed_text_calendar".to_string(),
        });
    }

    Ok(records)
}

fn read_status_records(
    backup_path: &Path,
    backup_id: &str,
) -> Result<Vec<StructuredRecord>, AdapterError> {
    let mut records = Vec::new();
    for (kind, folder, status) in [
        ("contact", "CONTACT", "encrypted_or_proprietary_payload"),
        ("calllog", "CALLLOG", "binary_or_proprietary_payload"),
    ] {
        let path = backup_path.join(folder);
        if path.exists() {
            records.push(StructuredRecord {
                id: format!("{backup_id}:{kind}:{}", path.to_string_lossy()),
                kind: kind.to_string(),
                title: folder.to_string(),
                subtitle: None,
                source_path: path.to_string_lossy().into_owned(),
                parse_status: status.to_string(),
            });
        }
    }
    Ok(records)
}

fn status_record(backup_id: &str, kind: &str, path: &Path, status: &str) -> StructuredRecord {
    StructuredRecord {
        id: format!("{backup_id}:{kind}:{}", path.to_string_lossy()),
        kind: kind.to_string(),
        title: path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or(kind)
            .to_string(),
        subtitle: None,
        source_path: path.to_string_lossy().into_owned(),
        parse_status: status.to_string(),
    }
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

    #[test]
    fn parses_text_calendar_records_and_statuses_proprietary_folders() {
        let temp = tempdir().unwrap();
        fs::create_dir_all(temp.path().join("CALENDER")).unwrap();
        fs::create_dir_all(temp.path().join("SAMSUNGNOTE")).unwrap();
        fs::write(temp.path().join("SAMSUNGNOTE/note.txt"), b"My note\nBody").unwrap();
        fs::write(
            temp.path().join("CALENDER/event.ics"),
            b"BEGIN:VCALENDAR\nSUMMARY:Lunch\nDTSTART:20260101T120000Z\nEND:VCALENDAR",
        )
        .unwrap();

        let records = read_structured_records(temp.path(), "backup").unwrap();
        assert!(records.iter().any(|item| item.title == "Lunch"));
        assert!(records.iter().any(|item| item.kind == "note"
            && item.parse_status == "parsed_text_note"
            && item.title == "My note"));
    }

    #[test]
    fn parses_readable_browser_records() {
        let temp = tempdir().unwrap();
        fs::create_dir_all(temp.path().join("SBROWSER")).unwrap();
        fs::write(
            temp.path().join("SBROWSER/bookmarks.json"),
            r#"{"children":[{"title":"Example","url":"https://example.com"}]}"#,
        )
        .unwrap();
        fs::write(
            temp.path().join("SBROWSER/history.html"),
            r#"<html><body><a href="https://example.org">Example Org</a></body></html>"#,
        )
        .unwrap();

        let records = read_structured_records(temp.path(), "backup").unwrap();
        assert!(records.iter().any(|item| item.kind == "browser"
            && item.title == "Example"
            && item.subtitle.as_deref() == Some("https://example.com")
            && item.parse_status == "parsed_browser_json"));
        assert!(records.iter().any(|item| item.kind == "browser"
            && item.title == "Example Org"
            && item.subtitle.as_deref() == Some("https://example.org")
            && item.parse_status == "parsed_browser_html"));
        assert!(!records
            .iter()
            .any(|item| item.kind == "browser" && item.parse_status == BROWSER_STATUS));
    }

    #[test]
    fn reports_encrypted_whatsapp_message_databases() {
        let temp = tempdir().unwrap();
        fs::create_dir_all(temp.path().join("MESSAGE/Databases")).unwrap();
        fs::write(
            temp.path().join("MESSAGE/Databases/msgstore.db.crypt14"),
            b"encrypted",
        )
        .unwrap();

        let records = read_structured_records(temp.path(), "backup").unwrap();
        assert!(records
            .iter()
            .any(|item| item.kind == "whatsapp_message"
                && item.parse_status == WHATSAPP_MESSAGE_STATUS));
    }
}
