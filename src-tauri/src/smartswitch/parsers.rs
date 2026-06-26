use crate::adapters::AdapterError;
use quick_xml::events::Event;
use quick_xml::name::QName;
use quick_xml::Reader;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Cursor, Read};
use std::path::Path;

use super::binary::parse_sdoc_text;
use super::{
    crypto, invalid_zip, open_zip, status_record, StructuredRecord, BROWSER_STATUS,
    WHATSAPP_MESSAGE_STATUS,
};

pub(super) fn read_contact_records(
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

pub(super) fn read_calllog_records(
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

pub(super) fn parse_calllog_xml(
    backup_id: &str,
    archive_path: &Path,
    xml: &[u8],
) -> Vec<StructuredRecord> {
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

pub(super) fn read_browser_records(
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

pub(super) fn read_app_records(
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

pub(super) fn read_whatsapp_message_records(
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

pub(super) fn read_note_records(
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

pub(super) fn read_calendar_records(
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

pub(super) fn read_status_records(
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
