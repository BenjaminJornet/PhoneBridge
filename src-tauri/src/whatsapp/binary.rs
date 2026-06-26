use crate::adapters::AdapterError;

use super::{KeyType, ParsedEncryptedDatabase};

pub(super) fn parse_encrypted_database(raw: &[u8]) -> Result<ParsedEncryptedDatabase<'_>, AdapterError> {
    let Some(size) = raw.first().copied() else {
        return Err(AdapterError::Parse("empty WhatsApp database".to_string()));
    };
    let mut cursor = 1_usize;
    if raw.get(cursor) == Some(&1) {
        cursor += 1;
    }
    let protobuf_end = cursor + size as usize;
    if protobuf_end > raw.len() {
        return Err(AdapterError::Parse(
            "invalid WhatsApp protobuf header".to_string(),
        ));
    }
    let header = &raw[..protobuf_end];
    let protobuf = &raw[cursor..protobuf_end];
    let (key_type, iv) = parse_backup_prefix(protobuf)?;
    if iv.len() != 16 {
        return Err(AdapterError::Parse(
            "WhatsApp IV must be 16 bytes".to_string(),
        ));
    }
    Ok(ParsedEncryptedDatabase {
        header,
        payload: &raw[protobuf_end..],
        iv,
        key_type,
    })
}

fn parse_backup_prefix(raw: &[u8]) -> Result<(KeyType, Vec<u8>), AdapterError> {
    let mut cursor = 0;
    let mut key_type = None;
    let mut c14 = None;
    let mut c15 = None;
    while cursor < raw.len() {
        let tag = read_varint(raw, &mut cursor)?;
        let field = tag >> 3;
        let wire = tag & 0b111;
        match (field, wire) {
            (1, 0) => key_type = Some(read_varint(raw, &mut cursor)?),
            (2, 2) => c14 = Some(read_len(raw, &mut cursor)?),
            (3, 2) => c15 = Some(read_len(raw, &mut cursor)?),
            (_, 0) => {
                let _ = read_varint(raw, &mut cursor)?;
            }
            (_, 2) => {
                let _ = read_len(raw, &mut cursor)?;
            }
            _ => {
                return Err(AdapterError::Parse(
                    "unsupported WhatsApp protobuf wire type".to_string(),
                ))
            }
        }
    }
    if let Some(bytes) = c15 {
        return Ok((KeyType::Crypt15, parse_nested_iv(bytes, 1)?));
    }
    if let Some(bytes) = c14 {
        return Ok((KeyType::Crypt14, parse_nested_iv(bytes, 5)?));
    }
    match key_type {
        Some(1) => Err(AdapterError::Parse("crypt15 IV missing".to_string())),
        _ => Err(AdapterError::Parse("crypt14 IV missing".to_string())),
    }
}

fn parse_nested_iv(raw: &[u8], iv_field: u64) -> Result<Vec<u8>, AdapterError> {
    let mut cursor = 0;
    while cursor < raw.len() {
        let tag = read_varint(raw, &mut cursor)?;
        let field = tag >> 3;
        let wire = tag & 0b111;
        match (field, wire) {
            (field, 2) if field == iv_field => return Ok(read_len(raw, &mut cursor)?.to_vec()),
            (_, 0) => {
                let _ = read_varint(raw, &mut cursor)?;
            }
            (_, 2) => {
                let _ = read_len(raw, &mut cursor)?;
            }
            _ => {
                return Err(AdapterError::Parse(
                    "unsupported WhatsApp nested protobuf wire type".to_string(),
                ))
            }
        }
    }
    Err(AdapterError::Parse(
        "WhatsApp IV field not found".to_string(),
    ))
}

fn read_varint(raw: &[u8], cursor: &mut usize) -> Result<u64, AdapterError> {
    let mut shift = 0;
    let mut value = 0_u64;
    while *cursor < raw.len() {
        let byte = raw[*cursor];
        *cursor += 1;
        value |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
        shift += 7;
        if shift > 63 {
            break;
        }
    }
    Err(AdapterError::Parse("invalid protobuf varint".to_string()))
}

fn read_len<'a>(raw: &'a [u8], cursor: &mut usize) -> Result<&'a [u8], AdapterError> {
    let len = read_varint(raw, cursor)? as usize;
    let end = cursor.saturating_add(len);
    if end > raw.len() {
        return Err(AdapterError::Parse(
            "protobuf length out of bounds".to_string(),
        ));
    }
    let bytes = &raw[*cursor..end];
    *cursor = end;
    Ok(bytes)
}
