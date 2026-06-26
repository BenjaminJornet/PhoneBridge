use std::fs;

use crate::adapters::AdapterError;
use crate::path_utils::expand_home;

use super::WhatsAppDecryptConfig;

pub(super) fn read_key_material(config: &WhatsAppDecryptConfig) -> Result<Vec<u8>, AdapterError> {
    if let Some(hex) = config
        .key_hex
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return decode_hex_key(hex);
    }
    let Some(path) = config.key_path.as_deref() else {
        return Err(AdapterError::Parse(
            "WhatsApp key file or 64-character key is required".to_string(),
        ));
    };
    normalize_key_file(&fs::read(expand_home(path))?)
}

pub(super) fn normalize_key_file(raw: &[u8]) -> Result<Vec<u8>, AdapterError> {
    if raw.len() == 131 || raw.len() == 32 {
        return Ok(raw.to_vec());
    }
    if let Ok(text) = std::str::from_utf8(raw) {
        let trimmed = text.trim();
        if trimmed.len() == 64 && trimmed.chars().all(|char| char.is_ascii_hexdigit()) {
            return decode_hex_key(trimmed);
        }
    }
    if let Some(key) = parse_java_serialized_byte_array(raw) {
        if key.len() == 131 || key.len() == 32 {
            return Ok(key);
        }
        return Err(AdapterError::Parse(format!(
            "unsupported WhatsApp Java key byte[] length: {}",
            key.len()
        )));
    }
    Err(AdapterError::Parse(
        "unrecognized WhatsApp key file format".to_string(),
    ))
}

fn parse_java_serialized_byte_array(raw: &[u8]) -> Option<Vec<u8>> {
    if !raw.starts_with(&[0xac, 0xed, 0x00, 0x05]) {
        return None;
    }
    let marker = b"[B";
    let marker_pos = raw
        .windows(marker.len())
        .position(|window| window == marker)?;
    let mut cursor = marker_pos + marker.len();
    skip_java_class_descriptor_tail(raw, &mut cursor)?;
    if raw.get(cursor) == Some(&0x70) {
        cursor += 1;
    }
    let len = read_be_u32(raw, cursor)? as usize;
    cursor += 4;
    raw.get(cursor..cursor + len).map(ToOwned::to_owned)
}

fn skip_java_class_descriptor_tail(raw: &[u8], cursor: &mut usize) -> Option<()> {
    *cursor += 9;
    let field_count = read_be_u16(raw, *cursor)? as usize;
    *cursor += 2;
    for _ in 0..field_count {
        let field_type = *raw.get(*cursor)?;
        *cursor += 1;
        let name_len = read_be_u16(raw, *cursor)? as usize;
        *cursor += 2 + name_len;
        if matches!(field_type, b'L' | b'[') {
            match raw.get(*cursor)? {
                0x74 => {
                    *cursor += 1;
                    let type_len = read_be_u16(raw, *cursor)? as usize;
                    *cursor += 2 + type_len;
                }
                0x71 => *cursor += 5,
                _ => return None,
            }
        }
    }
    loop {
        match raw.get(*cursor)? {
            0x78 => {
                *cursor += 1;
                return Some(());
            }
            0x77 => {
                *cursor += 1;
                let len = *raw.get(*cursor)? as usize;
                *cursor += 1 + len;
            }
            _ => return None,
        }
    }
}

fn read_be_u16(raw: &[u8], offset: usize) -> Option<u16> {
    let bytes = raw.get(offset..offset + 2)?;
    Some(u16::from_be_bytes([bytes[0], bytes[1]]))
}

fn read_be_u32(raw: &[u8], offset: usize) -> Option<u32> {
    let bytes = raw.get(offset..offset + 4)?;
    Some(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn decode_hex_key(hex: &str) -> Result<Vec<u8>, AdapterError> {
    if hex.len() != 64 || !hex.chars().all(|char| char.is_ascii_hexdigit()) {
        return Err(AdapterError::Parse(
            "WhatsApp hex key must be 64 hexadecimal characters".to_string(),
        ));
    }
    (0..hex.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&hex[index..index + 2], 16)
                .map_err(|err| AdapterError::Parse(err.to_string()))
        })
        .collect()
}
