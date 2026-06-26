use aes_gcm::aead::generic_array::typenum::U16;
use aes_gcm::AesGcm;
use hmac::Hmac;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::fs;
use std::path::Path;

use crate::adapters::AdapterError;
use crate::path_utils::expand_home;
use crate::smartswitch::StructuredRecord;

mod binary;
mod decryption;
mod keyderivation;

type Aes256Gcm16 = AesGcm<aes_gcm::aes::Aes256, U16>;
type HmacSha256 = Hmac<Sha256>;

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
    let key_material = keyderivation::read_key_material(&config)?;
    let parsed = binary::parse_encrypted_database(&encrypted)?;
    let key = decryption::derive_database_key(&key_material, parsed.key_type)?;
    let decrypted = decryption::decrypt_payload(&parsed, &key)?;
    let sqlite = decryption::normalize_plaintext(&decrypted)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::whatsapp::decryption::derive_database_key;
    use crate::whatsapp::keyderivation::normalize_key_file;
    use aes_gcm::aead::{Aead, KeyInit};
    use aes_gcm::Nonce;
    use md5::{Digest, Md5};
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
        assert_eq!(normalize_key_file(&key).unwrap(), key);
    }

    #[test]
    fn parses_java_serialized_byte_array_key_file() {
        let mut key = vec![0_u8; 131];
        key[0..3].copy_from_slice(&[0, 1, 1]);
        let serialized = java_serialized_byte_array(&key);

        assert_eq!(normalize_key_file(&serialized).unwrap(), key);
    }

    #[test]
    fn rejects_ambiguous_key_files() {
        let err = normalize_key_file(&[9_u8; 64]).unwrap_err().to_string();
        assert!(err.contains("unrecognized WhatsApp key file format"));
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

    fn java_serialized_byte_array(bytes: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&[0xac, 0xed, 0x00, 0x05]);
        out.push(0x75);
        out.push(0x72);
        out.extend_from_slice(&2_u16.to_be_bytes());
        out.extend_from_slice(b"[B");
        out.extend_from_slice(&[0xac, 0xf3, 0x17, 0xf8, 0x06, 0x08, 0x54, 0xe0]);
        out.extend_from_slice(&[0x02, 0x00, 0x00]);
        out.extend_from_slice(&[0x78, 0x70]);
        out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        out.extend_from_slice(bytes);
        out
    }
}
