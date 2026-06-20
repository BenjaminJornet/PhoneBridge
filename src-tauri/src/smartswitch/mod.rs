use crate::adapters::smartswitch::SmartSwitchAdapter;
use crate::adapters::{AdapterError, BackupAdapter};
use quick_xml::events::Event;
use quick_xml::name::QName;
use quick_xml::Reader;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Cursor, Read};
use std::path::{Path, PathBuf};
use zip::ZipArchive;

mod crypto;

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
        inventories.extend(read_folder_inventory(
            &source.id,
            &source.label,
            "APK",
            backup_path.join("APK"),
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
    records.extend(read_contact_records(backup_path, backup_id)?);
    records.extend(read_calllog_records(backup_path, backup_id)?);
    records.extend(read_browser_records(backup_path, backup_id)?);
    records.extend(read_app_records(backup_path, backup_id)?);
    records.extend(read_whatsapp_message_records(backup_path, backup_id)?);
    records.extend(read_status_records(
        backup_path,
        backup_id,
        records.iter().map(|record| record.kind.as_str()).collect(),
    )?);
    Ok(records)
}

fn read_contact_records(
    backup_path: &Path,
    backup_id: &str,
) -> Result<Vec<StructuredRecord>, AdapterError> {
    let archive_path = backup_path.join("CONTACT").join("Contact.SPBM");
    if !archive_path.exists() {
        return Ok(Vec::new());
    }

    let mut archive = open_zip(&archive_path)?;
    let mut records = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(invalid_zip)?;
        let name = entry.name().to_string();
        if !name.to_ascii_lowercase().ends_with(".json.enc") {
            continue;
        }
        let mut raw = Vec::new();
        entry.read_to_end(&mut raw)?;
        let decrypted = crypto::decrypt_iv_prefixed_payload(&raw)?;
        let json = crypto::extract_json_region(&decrypted)?;
        let parsed: serde_json::Value = serde_json::from_slice(json)?;
        collect_contact_records(backup_id, &archive_path, &parsed, &mut records);
    }

    Ok(records)
}

fn collect_contact_records(
    backup_id: &str,
    archive_path: &Path,
    value: &serde_json::Value,
    records: &mut Vec<StructuredRecord>,
) {
    match value {
        serde_json::Value::Array(items) => {
            for item in items {
                collect_contact_records(backup_id, archive_path, item, records);
            }
        }
        serde_json::Value::Object(map) => {
            let title = first_string(map, &["displayName", "name", "formattedName", "givenName"]);
            if let Some(title) = title {
                let subtitle = first_string(map, &["phoneNumber", "phone", "email", "number"]);
                records.push(StructuredRecord {
                    id: format!(
                        "{backup_id}:contact:{}:{}",
                        archive_path.to_string_lossy(),
                        records.len()
                    ),
                    kind: "contact".to_string(),
                    title,
                    subtitle,
                    source_path: archive_path.to_string_lossy().into_owned(),
                    parse_status: "parsed_decrypted_contact".to_string(),
                });
            }
            for child in map.values() {
                collect_contact_records(backup_id, archive_path, child, records);
            }
        }
        _ => {}
    }
}

fn read_calllog_records(
    backup_path: &Path,
    backup_id: &str,
) -> Result<Vec<StructuredRecord>, AdapterError> {
    let archive_path = backup_path.join("CALLLOG").join("CALLLOG.zip");
    if !archive_path.exists() {
        return Ok(Vec::new());
    }

    let mut archive = open_zip(&archive_path)?;
    let mut records = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(invalid_zip)?;
        let name = entry.name().to_ascii_lowercase();
        if !name.ends_with("call_log.exml") {
            continue;
        }
        let mut raw = Vec::new();
        entry.read_to_end(&mut raw)?;
        let decrypted = crypto::decrypt_iv_prefixed_payload(&raw)?;
        let xml = crypto::extract_xml_region(&decrypted, "CallLogs")?;
        records.extend(parse_calllog_xml(backup_id, &archive_path, xml));
    }
    Ok(records)
}

fn parse_calllog_xml(backup_id: &str, archive_path: &Path, xml: &[u8]) -> Vec<StructuredRecord> {
    let mut records = Vec::new();
    let mut reader = Reader::from_reader(Cursor::new(xml));
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) | Ok(Event::Empty(event))
                if event.name() == QName(b"CallLog") =>
            {
                let title = xml_attr(&reader, &event, b"name")
                    .or_else(|| xml_attr(&reader, &event, b"number"))
                    .or_else(|| xml_attr(&reader, &event, b"phoneNumber"))
                    .unwrap_or_else(|| "Call log".to_string());
                let subtitle = xml_attr(&reader, &event, b"date")
                    .or_else(|| xml_attr(&reader, &event, b"type"))
                    .or_else(|| xml_attr(&reader, &event, b"duration"));
                records.push(StructuredRecord {
                    id: format!(
                        "{backup_id}:calllog:{}:{}",
                        archive_path.to_string_lossy(),
                        records.len()
                    ),
                    kind: "calllog".to_string(),
                    title,
                    subtitle,
                    source_path: archive_path.to_string_lossy().into_owned(),
                    parse_status: "parsed_decrypted_calllog".to_string(),
                });
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buffer.clear();
    }

    records
}

fn xml_attr(
    reader: &Reader<Cursor<&[u8]>>,
    event: &quick_xml::events::BytesStart<'_>,
    key: &[u8],
) -> Option<String> {
    for attr in event.attributes().flatten() {
        if attr.key == QName(key) {
            if let Ok(value) = attr.decode_and_unescape_value(reader.decoder()) {
                return Some(value.into_owned());
            }
        }
    }
    None
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

fn read_app_records(
    backup_path: &Path,
    backup_id: &str,
) -> Result<Vec<StructuredRecord>, AdapterError> {
    let mut records = Vec::new();
    for (folder_name, status) in [
        ("APK", "inventory_apk_payload"),
        ("GALAXYSTORE", "inventory_app_store_payload"),
        ("DISABLEDAPPS", "inventory_disabled_apps_payload"),
    ] {
        let folder = backup_path.join(folder_name);
        if !folder.exists() {
            continue;
        }
        let mut found = false;
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
            if extension == "apk" {
                found = true;
                let size = entry.metadata()?.len();
                records.push(StructuredRecord {
                    id: format!("{backup_id}:app:{}", path.to_string_lossy()),
                    kind: "app".to_string(),
                    title: path
                        .file_stem()
                        .and_then(|value| value.to_str())
                        .unwrap_or("Android app")
                        .to_string(),
                    subtitle: Some(format!("{size} bytes")),
                    source_path: path.to_string_lossy().into_owned(),
                    parse_status: "parsed_apk_inventory".to_string(),
                });
            }
        }
        if !found {
            records.push(status_record(backup_id, "app", &folder, status));
        }
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
        } else if matches!(extension.as_str(), "sdoc" | "sdocx") {
            match parse_sdoc_text(path) {
                Ok(Some(text)) => {
                    let title = text
                        .lines()
                        .find(|line| !line.trim().is_empty())
                        .map(|line| line.trim().chars().take(80).collect::<String>())
                        .unwrap_or_else(|| "Samsung note".to_string());
                    records.push(StructuredRecord {
                        id: format!("{backup_id}:note:{}", path.to_string_lossy()),
                        kind: "note".to_string(),
                        title,
                        subtitle: Some(format!("{} characters", text.chars().count())),
                        source_path: path.to_string_lossy().into_owned(),
                        parse_status: "parsed_sdocx_text".to_string(),
                    });
                }
                Ok(None) | Err(_) => records.push(status_record(
                    backup_id,
                    "note",
                    path,
                    "proprietary_note_payload",
                )),
            }
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

fn parse_sdoc_text(path: &Path) -> Result<Option<String>, AdapterError> {
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file).map_err(invalid_zip)?;
    let mut note = None;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(invalid_zip)?;
        if entry.name().ends_with("note.note") {
            let mut raw = Vec::new();
            entry.read_to_end(&mut raw)?;
            note = Some(raw);
            break;
        }
    }
    let Some(raw) = note else {
        return Ok(None);
    };
    let mut text = extract_sdoc_note_text(&raw).unwrap_or_default();
    text.push_str(&extract_sdoc_page_text(path)?);
    if text.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(text))
    }
}

fn extract_sdoc_page_text(path: &Path) -> Result<String, AdapterError> {
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file).map_err(invalid_zip)?;
    let mut output = String::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(invalid_zip)?;
        if !entry.name().ends_with(".page") {
            continue;
        }
        let mut raw = Vec::new();
        entry.read_to_end(&mut raw)?;
        for text in extract_text_records_from_blob(&raw) {
            if !output.is_empty() {
                output.push('\n');
            }
            output.push_str(&text);
        }
    }
    Ok(output)
}

fn extract_sdoc_note_text(data: &[u8]) -> Result<String, AdapterError> {
    let mut cursor = Cursor::new(data);
    cursor.set_position(14);
    let _format_version = read_u32(&mut cursor)?;
    let note_id_len = read_u16(&mut cursor)? as u64;
    cursor.set_position(cursor.position() + note_id_len * 2);
    cursor.set_position(cursor.position() + 4 + 8 + 8 + 4 + 4 + 4 + 4 + 4);
    let title_size = read_u32(&mut cursor)? as u64;
    cursor.set_position(cursor.position() + title_size);
    let body_size = read_u32(&mut cursor)? as usize;
    let body_start = cursor.position() as usize;
    let body_end = body_start.saturating_add(body_size);
    if body_end > data.len() {
        return Err(AdapterError::Parse(
            "sdocx body exceeds note length".to_string(),
        ));
    }
    let body = &data[body_start..body_end];
    extract_text_common_text(body)
}

fn extract_text_common_text(body: &[u8]) -> Result<String, AdapterError> {
    let object_base_size = read_u32_at(body, 0)? as usize;
    let shape_base_size = read_u32_at(body, object_base_size)? as usize;
    let shape_text_start = object_base_size + shape_base_size;
    let shape_text_size = read_u32_at(body, shape_text_start)? as usize;
    let shape_text_end = shape_text_start.saturating_add(shape_text_size);
    if shape_text_end > body.len() || shape_text_start + 14 > body.len() {
        return Err(AdapterError::Parse("invalid sdocx text record".to_string()));
    }
    let record_type = read_u16_at(body, shape_text_start + 4)?;
    if record_type != 7 {
        return Err(AdapterError::Parse(
            "sdocx text record not found".to_string(),
        ));
    }
    let own_data_offset = read_u32_at(body, shape_text_start + 6)? as usize;
    let text_common_offset = shape_text_start + 4 + own_data_offset;
    let _text_common_size = read_u32_at(body, text_common_offset)? as usize;
    let text_len = read_u32_at(body, text_common_offset + 4)? as usize;
    let text_start = text_common_offset + 8;
    let text_end = text_start.saturating_add(text_len * 2);
    if text_end > body.len() {
        return Err(AdapterError::Parse(
            "sdocx text exceeds body length".to_string(),
        ));
    }
    utf16le_to_string(&body[text_start..text_end])
}

fn extract_text_records_from_blob(blob: &[u8]) -> Vec<String> {
    let mut texts = Vec::new();
    for offset in 0..blob.len().saturating_sub(14) {
        let Ok(record_type) = read_u16_at(blob, offset + 4) else {
            continue;
        };
        if record_type != 7 {
            continue;
        }
        let Ok(size) = read_u32_at(blob, offset) else {
            continue;
        };
        let Ok(own_data_offset) = read_u32_at(blob, offset + 6) else {
            continue;
        };
        let text_common_offset = offset + 4 + own_data_offset as usize;
        let Ok(_text_common_size) = read_u32_at(blob, text_common_offset) else {
            continue;
        };
        let Ok(text_len) = read_u32_at(blob, text_common_offset + 4) else {
            continue;
        };
        let text_start = text_common_offset + 8;
        let text_end = text_start.saturating_add(text_len as usize * 2);
        if text_end > blob.len() || offset + size as usize > blob.len() {
            continue;
        }
        if let Ok(text) = utf16le_to_string(&blob[text_start..text_end]) {
            if !text.trim().is_empty() && !texts.contains(&text) {
                texts.push(text);
            }
        }
    }
    texts
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
    parsed_kinds: Vec<&str>,
) -> Result<Vec<StructuredRecord>, AdapterError> {
    let mut records = Vec::new();
    for (kind, folder, status) in [
        ("contact", "CONTACT", "encrypted_or_proprietary_payload"),
        ("calllog", "CALLLOG", "binary_or_proprietary_payload"),
        ("app", "APK", "binary_apk_payload"),
    ] {
        let path = backup_path.join(folder);
        if path.exists() {
            if parsed_kinds.contains(&kind) {
                continue;
            }
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

fn open_zip(path: &Path) -> Result<ZipArchive<File>, AdapterError> {
    let file = File::open(path)?;
    ZipArchive::new(file).map_err(invalid_zip)
}

fn invalid_zip(err: zip::result::ZipError) -> AdapterError {
    AdapterError::Filesystem(std::io::Error::new(std::io::ErrorKind::InvalidData, err))
}

fn read_u16<R: Read>(reader: &mut R) -> Result<u16, AdapterError> {
    let mut buffer = [0_u8; 2];
    reader.read_exact(&mut buffer)?;
    Ok(u16::from_le_bytes(buffer))
}

fn read_u32<R: Read>(reader: &mut R) -> Result<u32, AdapterError> {
    let mut buffer = [0_u8; 4];
    reader.read_exact(&mut buffer)?;
    Ok(u32::from_le_bytes(buffer))
}

fn read_u16_at(data: &[u8], offset: usize) -> Result<u16, AdapterError> {
    let bytes = data
        .get(offset..offset + 2)
        .ok_or_else(|| AdapterError::Parse("u16 out of bounds".to_string()))?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u32_at(data: &[u8], offset: usize) -> Result<u32, AdapterError> {
    let bytes = data
        .get(offset..offset + 4)
        .ok_or_else(|| AdapterError::Parse("u32 out of bounds".to_string()))?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn utf16le_to_string(data: &[u8]) -> Result<String, AdapterError> {
    if data.len() % 2 != 0 {
        return Err(AdapterError::Parse("odd utf16 length".to_string()));
    }
    let units: Vec<u16> = data
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();
    String::from_utf16(&units).map_err(|err| AdapterError::Parse(err.to_string()))
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
    use aes::Aes128;
    use cbc::cipher::{block_padding::NoPadding, BlockEncryptMut, KeyIvInit};
    use std::io::Write;
    use tempfile::tempdir;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

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

    #[test]
    fn inventories_apk_and_app_status_folders() {
        let temp = tempdir().unwrap();
        fs::create_dir_all(temp.path().join("APK/apps")).unwrap();
        fs::create_dir_all(temp.path().join("DISABLEDAPPS")).unwrap();
        fs::write(temp.path().join("APK/apps/example.apk"), b"apk").unwrap();

        let records = read_structured_records(temp.path(), "backup").unwrap();
        assert!(records.iter().any(|item| item.kind == "app"
            && item.title == "example"
            && item.parse_status == "parsed_apk_inventory"));
        assert!(records.iter().any(|item| item.kind == "app"
            && item.title == "DISABLEDAPPS"
            && item.parse_status == "inventory_disabled_apps_payload"));
    }

    #[test]
    fn parses_decrypted_contact_and_calllog_records() {
        let temp = tempdir().unwrap();
        fs::create_dir_all(temp.path().join("CONTACT")).unwrap();
        fs::create_dir_all(temp.path().join("CALLLOG")).unwrap();

        write_zip(
            &temp.path().join("CONTACT/Contact.SPBM"),
            "CONTACT_JSON/contact.json.enc",
            &encrypt_fixture(br#"[{"displayName":"Ada Lovelace","phoneNumber":"+100"}]"#),
        );
        write_zip(
            &temp.path().join("CALLLOG/CALLLOG.zip"),
            "call_log.exml",
            &encrypt_fixture(br#"<?xml version="1.0"?><CallLogs><CallLog name="Ada" number="+100" date="2026" /></CallLogs>"#),
        );

        let records = read_structured_records(temp.path(), "backup").unwrap();
        assert!(records.iter().any(|item| item.kind == "contact"
            && item.title == "Ada Lovelace"
            && item.parse_status == "parsed_decrypted_contact"));
        assert!(records.iter().any(|item| item.kind == "calllog"
            && item.title == "Ada"
            && item.parse_status == "parsed_decrypted_calllog"));
        assert!(!records.iter().any(|item| item.kind == "contact"
            && item.parse_status == "encrypted_or_proprietary_payload"));
        assert!(!records
            .iter()
            .any(|item| item.kind == "calllog"
                && item.parse_status == "binary_or_proprietary_payload"));
    }

    #[test]
    fn preserves_calllog_attributes_with_spaces_and_quotes() {
        let records = parse_calllog_xml(
            "backup",
            Path::new("/tmp/CALLLOG.zip"),
            br#"<?xml version="1.0"?><CallLogs><CallLog name="Ada Lovelace" number="+100" type="Missed call" date="2026-06-21 &quot;noon&quot;" /></CallLogs>"#,
        );

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].title, "Ada Lovelace");
        assert_eq!(records[0].subtitle.as_deref(), Some("2026-06-21 \"noon\""));
    }

    #[test]
    fn parses_real_shape_decrypted_contact_fixture() {
        let temp = tempdir().unwrap();
        fs::create_dir_all(temp.path().join("CONTACT")).unwrap();
        write_zip(
            &temp.path().join("CONTACT/Contact.SPBM"),
            "CONTACT_JSON.zip/contact_0001.json.enc",
            &encrypt_fixture(
                br#"{"contacts":[{"formattedName":"Anon Person","email":"anon@example.test"}]}"#,
            ),
        );

        let records = read_structured_records(temp.path(), "backup").unwrap();
        assert!(records.iter().any(|item| item.kind == "contact"
            && item.title == "Anon Person"
            && item.subtitle.as_deref() == Some("anon@example.test")));
    }

    #[test]
    fn extracts_text_from_sdocx_note() {
        let temp = tempdir().unwrap();
        fs::create_dir_all(temp.path().join("SAMSUNGNOTE")).unwrap();
        let note_path = temp.path().join("SAMSUNGNOTE/note.sdocx");
        write_sdocx_fixture(&note_path, "Typed note\nBody", "Canvas text box");

        let records = read_structured_records(temp.path(), "backup").unwrap();
        assert!(records.iter().any(|item| item.kind == "note"
            && item.title == "Typed note"
            && item.parse_status == "parsed_sdocx_text"));
        assert!(records
            .iter()
            .any(|item| item.kind == "note" && item.subtitle.as_deref() == Some("30 characters")));
    }

    fn write_sdocx_fixture(path: &Path, body_text: &str, page_text: &str) {
        let file = File::create(path).unwrap();
        let mut zip = ZipWriter::new(file);
        zip.start_file("note.note", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&sdoc_note_fixture(body_text)).unwrap();
        zip.start_file("page-1.page", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&sdoc_page_fixture(page_text)).unwrap();
        zip.finish().unwrap();
    }

    fn write_zip(path: &Path, name: &str, bytes: &[u8]) {
        let file = File::create(path).unwrap();
        let mut zip = ZipWriter::new(file);
        zip.start_file(name, SimpleFileOptions::default()).unwrap();
        zip.write_all(bytes).unwrap();
        zip.finish().unwrap();
    }

    fn encrypt_fixture(plaintext: &[u8]) -> Vec<u8> {
        let iv = [3_u8; 16];
        let mut padded = plaintext.to_vec();
        let aligned_len = padded.len().next_multiple_of(16);
        padded.resize(aligned_len, 0);
        let key = crypto::derive_dummy_key();
        let mut buffer = padded.clone();
        let ciphertext = cbc::Encryptor::<Aes128>::new_from_slices(&key, &iv)
            .unwrap()
            .encrypt_padded_mut::<NoPadding>(&mut buffer, padded.len())
            .unwrap();

        let mut raw = iv.to_vec();
        raw.extend_from_slice(ciphertext);
        raw
    }

    fn sdoc_note_fixture(text: &str) -> Vec<u8> {
        let mut body = Vec::new();
        append_u32(&mut body, 8);
        body.extend_from_slice(&[0; 4]);
        append_u32(&mut body, 8);
        body.extend_from_slice(&[0; 4]);

        let text_units: Vec<u16> = text.encode_utf16().collect();
        let text_common_size = 8 + text_units.len() * 2;
        let own_data_offset = 12_u32;
        let shape_text_size = own_data_offset as usize + text_common_size;
        append_u32(&mut body, shape_text_size as u32);
        append_u16(&mut body, 7);
        append_u32(&mut body, own_data_offset);
        body.extend_from_slice(&[0; 6]);
        append_u32(&mut body, text_common_size as u32);
        append_u32(&mut body, text_units.len() as u32);
        for unit in text_units {
            append_u16(&mut body, unit);
        }

        let mut note = vec![0; 14];
        append_u32(&mut note, 4000);
        append_u16(&mut note, 0);
        append_u32(&mut note, 1);
        note.extend_from_slice(&0_u64.to_le_bytes());
        note.extend_from_slice(&0_u64.to_le_bytes());
        append_u32(&mut note, 100);
        append_u32(&mut note, 100);
        append_u32(&mut note, 0);
        append_u32(&mut note, 0);
        append_u32(&mut note, 4000);
        append_u32(&mut note, 0);
        append_u32(&mut note, body.len() as u32);
        note.extend_from_slice(&body);
        note
    }

    fn sdoc_page_fixture(text: &str) -> Vec<u8> {
        let mut blob = vec![0xaa; 20];
        append_shape_text(&mut blob, text);
        blob.extend_from_slice(&[0xbb; 20]);
        blob
    }

    fn append_shape_text(bytes: &mut Vec<u8>, text: &str) {
        let text_units: Vec<u16> = text.encode_utf16().collect();
        let text_common_size = 8 + text_units.len() * 2;
        let own_data_offset = 12_u32;
        let shape_text_size = own_data_offset as usize + text_common_size;
        append_u32(bytes, shape_text_size as u32);
        append_u16(bytes, 7);
        append_u32(bytes, own_data_offset);
        bytes.extend_from_slice(&[0; 6]);
        append_u32(bytes, text_common_size as u32);
        append_u32(bytes, text_units.len() as u32);
        for unit in text_units {
            append_u16(bytes, unit);
        }
    }

    fn append_u16(bytes: &mut Vec<u8>, value: u16) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn append_u32(bytes: &mut Vec<u8>, value: u32) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
}
