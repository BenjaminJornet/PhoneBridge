use crate::adapters::smartswitch::SmartSwitchAdapter;
use crate::adapters::{AdapterError, BackupAdapter};
use serde::Serialize;
use std::fs::File;
use std::path::Path;
use zip::ZipArchive;

mod binary;
mod crypto;
mod inventory;
mod parsers;

use inventory::{read_archive_inventory, read_folder_inventory, read_item_metrics};
use parsers::{
    read_app_records, read_browser_records, read_calendar_records, read_calllog_records,
    read_contact_records, read_note_records, read_status_records, read_whatsapp_message_records,
};

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

pub(super) fn open_zip(path: &Path) -> Result<ZipArchive<File>, AdapterError> {
    let file = File::open(path)?;
    ZipArchive::new(file).map_err(invalid_zip)
}

pub(super) fn invalid_zip(err: zip::result::ZipError) -> AdapterError {
    AdapterError::Filesystem(std::io::Error::new(std::io::ErrorKind::InvalidData, err))
}

pub(super) fn status_record(
    backup_id: &str,
    kind: &str,
    path: &Path,
    status: &str,
) -> StructuredRecord {
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

#[cfg(test)]
mod tests {
    use super::inventory::looks_like_text_xml;
    use super::parsers::parse_calllog_xml;
    use super::*;
    use aes::Aes128;
    use cbc::cipher::{block_padding::NoPadding, BlockEncryptMut, KeyIvInit};
    use std::fs;
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
