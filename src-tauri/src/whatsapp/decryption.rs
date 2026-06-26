use aes_gcm::aead::generic_array::typenum::U16;
use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::Nonce;
use flate2::read::ZlibDecoder;
use hmac::Mac;
use md5::{Digest, Md5};
use std::io::Read;

use crate::adapters::AdapterError;

use super::{Aes256Gcm16, HmacSha256, KeyType, ParsedEncryptedDatabase};

pub(super) fn derive_database_key(
    key_material: &[u8],
    key_type: KeyType,
) -> Result<[u8; 32], AdapterError> {
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

pub(super) fn decrypt_payload(
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

pub(super) fn normalize_plaintext(decrypted: &[u8]) -> Result<Vec<u8>, AdapterError> {
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
