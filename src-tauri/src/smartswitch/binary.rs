use crate::adapters::AdapterError;
use std::fs::File;
use std::io::{Cursor, Read};
use std::path::Path;
use zip::ZipArchive;

use super::invalid_zip;

pub(super) fn parse_sdoc_text(path: &Path) -> Result<Option<String>, AdapterError> {
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
