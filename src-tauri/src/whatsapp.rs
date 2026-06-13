use aes_gcm::aead::generic_array::typenum::U16;
use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{AesGcm, Nonce};
use flate2::read::ZlibDecoder;
use hmac::{Hmac, Mac};
use md5::{Digest, Md5};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::adapters::AdapterError;
use crate::smartswitch::StructuredRecord;

type Aes256Gcm16 = AesGcm<aes_gcm::aes::Aes256, U16>;
type HmacSha256 = Hmac<Sha256>;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WhatsAppDecryptConfig {
    pub encrypted_db_path: String,
    pub key_path: Option<String>,
    pub key_hex: Option<String>,
    pub output_path: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WhatsAppDecryptResult {
    pub output_path: String,
    pub message_count: u64,
    pub chat_count: u64,
    pub records: Vec<StructuredRecord>,
}

pub fn decrypt_whatsapp_database(
    config: WhatsAppDecryptConfig,
) -> Result<WhatsAppDecryptResult, AdapterError> {
    let encrypted = fs::read(expand_home(&config.encrypted_db_path))?;
    let key_material = read_key_material(&config)?;
    let parsed = parse_encrypted_database(&encrypted)?;
    let key = derive_database_key(&key_material, parsed.key_type)?;
    let decrypted = decrypt_payload(&parsed, &key)?;
    let sqlite = normalize_plaintext(&decrypted)?;
    let output_path = expand_home(&config.output_path);
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&output_path, &sqlite)?;
    let records = read_message_records(&output_path)?;
    let message_count = count_table_rows(&output_path, "messages").unwrap_or(records.len() as u64);
    let chat_count = count_table_rows(&output_path, "chat_list").unwrap_or(0);

    Ok(WhatsAppDecryptResult {
        output_path: output_path.to_string_lossy().into_owned(),
        message_count,
        chat_count,
        records,
    })
}

fn read_key_material(config: &WhatsAppDecryptConfig) -> Result<Vec<u8>, AdapterError> {
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

fn normalize_key_file(raw: &[u8]) -> Result<Vec<u8>, AdapterError> {
    if raw.len() == 131 || raw.len() == 32 {
        return Ok(raw.to_vec());
    }
    if let Ok(text) = std::str::from_utf8(raw) {
        let trimmed = text.trim();
        if trimmed.len() == 64 && trimmed.chars().all(|char| char.is_ascii_hexdigit()) {
            return decode_hex_key(trimmed);
        }
    }
    if let Some(key) = find_crypt14_key(raw) {
        return Ok(key.to_vec());
    }
    if raw.len() > 32 {
        return Ok(raw[raw.len() - 32..].to_vec());
    }
    Err(AdapterError::Parse(
        "unrecognized WhatsApp key file format".to_string(),
    ))
}

fn find_crypt14_key(raw: &[u8]) -> Option<&[u8]> {
    raw.windows(131).find(|candidate| {
        candidate.starts_with(&[0, 1])
            && matches!(candidate.get(2), Some(1..=3))
            && Sha256::digest(&candidate[35..51]).as_slice() == &candidate[51..83]
    })
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KeyType {
    Crypt14,
    Crypt15,
}

struct ParsedEncryptedDatabase<'a> {
    header: &'a [u8],
    payload: &'a [u8],
    iv: Vec<u8>,
    key_type: KeyType,
}

fn parse_encrypted_database(raw: &[u8]) -> Result<ParsedEncryptedDatabase<'_>, AdapterError> {
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

fn derive_database_key(key_material: &[u8], key_type: KeyType) -> Result<[u8; 32], AdapterError> {
    match key_type {
        KeyType::Crypt14 => {
            if key_material.len() != 131 {
                return Err(AdapterError::Parse(
                    "crypt14 requires a 131-byte key payload".to_string(),
                ));
            }
            let mut key = [0_u8; 32];
            key.copy_from_slice(&key_material[99..131]);
            Ok(key)
        }
        KeyType::Crypt15 => {
            if key_material.len() != 32 {
                return Err(AdapterError::Parse(
                    "crypt15 requires a 32-byte root key".to_string(),
                ));
            }
            let private_key = hmac_sha256(&[0_u8; 32], key_material)?;
            hmac_sha256(&private_key, b"backup encryption\x01")
        }
    }
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> Result<[u8; 32], AdapterError> {
    let mut mac = <HmacSha256 as Mac>::new_from_slice(key)
        .map_err(|err| AdapterError::Parse(err.to_string()))?;
    mac.update(message);
    let bytes = mac.finalize().into_bytes();
    let mut out = [0_u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

fn decrypt_payload(
    parsed: &ParsedEncryptedDatabase<'_>,
    key: &[u8; 32],
) -> Result<Vec<u8>, AdapterError> {
    if parsed.payload.len() < 16 {
        return Err(AdapterError::Parse(
            "WhatsApp encrypted payload too small".to_string(),
        ));
    }
    let cipher =
        Aes256Gcm16::new_from_slice(key).map_err(|err| AdapterError::Parse(err.to_string()))?;
    let nonce = Nonce::<U16>::from_slice(&parsed.iv);
    if parsed.payload.len() >= 32 {
        let checksum = &parsed.payload[parsed.payload.len() - 16..];
        let tag = &parsed.payload[parsed.payload.len() - 32..parsed.payload.len() - 16];
        let ciphertext = &parsed.payload[..parsed.payload.len() - 32];
        let mut md5 = Md5::new();
        md5.update(parsed.header);
        md5.update(ciphertext);
        md5.update(tag);
        if md5.finalize().as_slice() == checksum {
            let mut encrypted = ciphertext.to_vec();
            encrypted.extend_from_slice(tag);
            return cipher
                .decrypt(
                    nonce,
                    Payload {
                        msg: &encrypted,
                        aad: &[],
                    },
                )
                .map_err(|_| {
                    AdapterError::Parse("WhatsApp GCM authentication failed".to_string())
                });
        }
    }
    cipher
        .decrypt(nonce, parsed.payload)
        .map_err(|_| AdapterError::Parse("WhatsApp GCM authentication failed".to_string()))
}

fn normalize_plaintext(decrypted: &[u8]) -> Result<Vec<u8>, AdapterError> {
    if decrypted.starts_with(b"SQLite format 3") || decrypted.starts_with(b"PK\x03\x04") {
        return Ok(decrypted.to_vec());
    }
    let mut decoder = ZlibDecoder::new(decrypted);
    let mut out = Vec::new();
    decoder.read_to_end(&mut out)?;
    if !out.starts_with(b"SQLite format 3") {
        return Err(AdapterError::Parse(
            "decrypted WhatsApp payload is not SQLite".to_string(),
        ));
    }
    Ok(out)
}

fn read_message_records(path: &Path) -> Result<Vec<StructuredRecord>, AdapterError> {
    let connection = Connection::open(path).map_err(|err| AdapterError::Parse(err.to_string()))?;
    let mut records = Vec::new();
    if !table_exists(&connection, "messages")? {
        return Ok(records);
    }
    let mut statement = connection
        .prepare(
            "SELECT key_remote_jid, data, timestamp FROM messages WHERE data IS NOT NULL LIMIT 100",
        )
        .map_err(|err| AdapterError::Parse(err.to_string()))?;
    let rows = statement
        .query_map([], |row| {
            let jid: Option<String> = row.get(0)?;
            let data: Option<String> = row.get(1)?;
            let timestamp: Option<i64> = row.get(2)?;
            Ok((jid, data, timestamp))
        })
        .map_err(|err| AdapterError::Parse(err.to_string()))?;
    for (index, row) in rows.enumerate() {
        let (jid, data, timestamp) = row.map_err(|err| AdapterError::Parse(err.to_string()))?;
        let title = data.unwrap_or_else(|| "WhatsApp message".to_string());
        records.push(StructuredRecord {
            id: format!("whatsapp-message:{}:{index}", path.to_string_lossy()),
            kind: "whatsapp_message".to_string(),
            title: title.chars().take(120).collect(),
            subtitle: jid.or_else(|| timestamp.map(|value| value.to_string())),
            source_path: path.to_string_lossy().into_owned(),
            parse_status: "parsed_decrypted_whatsapp_message".to_string(),
        });
    }
    Ok(records)
}

fn count_table_rows(path: &Path, table: &str) -> Result<u64, AdapterError> {
    let connection = Connection::open(path).map_err(|err| AdapterError::Parse(err.to_string()))?;
    if !table_exists(&connection, table)? {
        return Ok(0);
    }
    let query = format!("SELECT COUNT(*) FROM {table}");
    let count: i64 = connection
        .query_row(&query, [], |row| row.get(0))
        .map_err(|err| AdapterError::Parse(err.to_string()))?;
    Ok(count as u64)
}

fn table_exists(connection: &Connection, table: &str) -> Result<bool, AdapterError> {
    let exists: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
            |row| row.get(0),
        )
        .map_err(|err| AdapterError::Parse(err.to_string()))?;
    Ok(exists > 0)
}

fn expand_home(path: &str) -> PathBuf {
    if path == "~" {
        return std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(path));
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join(rest);
    }
    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aes_gcm::aead::Aead;
    use rusqlite::params;
    use tempfile::tempdir;

    #[test]
    fn derives_crypt14_key_from_key_payload() {
        let mut key = vec![0_u8; 131];
        key[0..3].copy_from_slice(&[0, 1, 1]);
        key[35..51].copy_from_slice(&[9_u8; 16]);
        let digest = Sha256::digest(&key[35..51]);
        key[51..83].copy_from_slice(&digest);
        key[99..131].copy_from_slice(&[7_u8; 32]);
        assert_eq!(
            derive_database_key(&key, KeyType::Crypt14).unwrap(),
            [7_u8; 32]
        );
        assert!(find_crypt14_key(&key).is_some());
    }

    #[test]
    fn decrypts_crypt15_fixture_to_sqlite_messages() {
        let temp = tempdir().unwrap();
        let sqlite_path = temp.path().join("msgstore.db");
        create_message_db(&sqlite_path);
        let sqlite = fs::read(&sqlite_path).unwrap();
        let compressed = compress_zlib(&sqlite);
        let root_key = [4_u8; 32];
        let db_key = derive_database_key(&root_key, KeyType::Crypt15).unwrap();
        let iv = [5_u8; 16];
        let encrypted = encrypt_fixture(&compressed, &db_key, &iv);
        let encrypted_path = temp.path().join("msgstore.db.crypt15");
        fs::write(&encrypted_path, encrypted).unwrap();
        let key_path = temp.path().join("encrypted_backup.key");
        fs::write(&key_path, root_key).unwrap();
        let output_path = temp.path().join("out.db");

        let result = decrypt_whatsapp_database(WhatsAppDecryptConfig {
            encrypted_db_path: encrypted_path.to_string_lossy().into_owned(),
            key_path: Some(key_path.to_string_lossy().into_owned()),
            key_hex: None,
            output_path: output_path.to_string_lossy().into_owned(),
        })
        .unwrap();

        assert_eq!(result.message_count, 1);
        assert_eq!(result.records[0].title, "hello from whatsapp");
        assert!(fs::read(output_path)
            .unwrap()
            .starts_with(b"SQLite format 3"));
    }

    fn create_message_db(path: &Path) {
        let connection = Connection::open(path).unwrap();
        connection
            .execute(
                "CREATE TABLE messages (key_remote_jid TEXT, data TEXT, timestamp INTEGER)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO messages (key_remote_jid, data, timestamp) VALUES (?1, ?2, ?3)",
                params!["chat@s.whatsapp.net", "hello from whatsapp", 1_i64],
            )
            .unwrap();
    }

    fn compress_zlib(bytes: &[u8]) -> Vec<u8> {
        use flate2::write::ZlibEncoder;
        use flate2::Compression;
        use std::io::Write;
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(bytes).unwrap();
        encoder.finish().unwrap()
    }

    fn encrypt_fixture(plaintext: &[u8], key: &[u8; 32], iv: &[u8; 16]) -> Vec<u8> {
        let nested = encode_len_field(1, iv);
        let prefix = [
            encode_varint((1 << 3) | 0),
            encode_varint(1),
            encode_len_field(3, &nested),
        ]
        .concat();
        let mut header = vec![prefix.len() as u8];
        header.extend_from_slice(&prefix);
        let cipher = Aes256Gcm16::new_from_slice(key).unwrap();
        let encrypted = cipher
            .encrypt(Nonce::<U16>::from_slice(iv), plaintext)
            .unwrap();
        let (ciphertext, tag) = encrypted.split_at(encrypted.len() - 16);
        let mut checksum = Md5::new();
        checksum.update(&header);
        checksum.update(ciphertext);
        checksum.update(tag);
        let mut out = header;
        out.extend_from_slice(ciphertext);
        out.extend_from_slice(tag);
        out.extend_from_slice(&checksum.finalize());
        out
    }

    fn encode_len_field(field: u64, bytes: &[u8]) -> Vec<u8> {
        [
            encode_varint((field << 3) | 2),
            encode_varint(bytes.len() as u64),
            bytes.to_vec(),
        ]
        .concat()
    }

    fn encode_varint(mut value: u64) -> Vec<u8> {
        let mut out = Vec::new();
        while value >= 0x80 {
            out.push((value as u8) | 0x80);
            value >>= 7;
        }
        out.push(value as u8);
        out
    }
}
