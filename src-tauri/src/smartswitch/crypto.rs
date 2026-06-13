use crate::adapters::AdapterError;
use aes::Aes128;
use cbc::cipher::{block_padding::NoPadding, BlockDecryptMut, KeyIvInit};
use sha2::{Digest, Sha256};

const DUMMY_HEX: &str = "9AB412D3C1F2EF658BFC0CFFCCC344D44C0A";

pub fn derive_dummy_key() -> [u8; 16] {
    let digest = Sha256::digest(DUMMY_HEX.as_bytes());
    let mut key = [0_u8; 16];
    key.copy_from_slice(&digest[..16]);
    key
}

pub fn decrypt_iv_prefixed_payload(raw: &[u8]) -> Result<Vec<u8>, AdapterError> {
    decrypt_with_tail_trim(raw, 0).or_else(|_| decrypt_with_tail_trim(raw, 16))
}

fn decrypt_with_tail_trim(raw: &[u8], trim_tail_bytes: usize) -> Result<Vec<u8>, AdapterError> {
    if raw.len() < 32 {
        return Err(AdapterError::Parse(
            "encrypted payload too small".to_string(),
        ));
    }
    let iv = &raw[..16];
    let mut tail = &raw[16..];
    if trim_tail_bytes > 0 {
        if tail.len() <= trim_tail_bytes {
            return Err(AdapterError::Parse(
                "encrypted payload tail is too short".to_string(),
            ));
        }
        tail = &tail[..tail.len() - trim_tail_bytes];
    }
    let aligned_len = tail.len() - (tail.len() % 16);
    if aligned_len == 0 {
        return Err(AdapterError::Parse(
            "encrypted payload has no aligned ciphertext".to_string(),
        ));
    }

    let mut ciphertext = tail[..aligned_len].to_vec();
    let key = derive_dummy_key();
    let decrypted = cbc::Decryptor::<Aes128>::new_from_slices(&key, iv)
        .map_err(|err| AdapterError::Parse(err.to_string()))?
        .decrypt_padded_mut::<NoPadding>(&mut ciphertext)
        .map_err(|err| AdapterError::Parse(err.to_string()))?;
    Ok(decrypted.to_vec())
}

pub fn extract_json_region(payload: &[u8]) -> Result<&[u8], AdapterError> {
    let start_array = payload.iter().position(|byte| *byte == b'[');
    let start_object = payload.iter().position(|byte| *byte == b'{');
    let start = [start_array, start_object]
        .into_iter()
        .flatten()
        .min()
        .ok_or_else(|| AdapterError::Parse("JSON start not found".to_string()))?;
    let end_array = payload.iter().rposition(|byte| *byte == b']');
    let end_object = payload.iter().rposition(|byte| *byte == b'}');
    let end = [end_array, end_object]
        .into_iter()
        .flatten()
        .max()
        .ok_or_else(|| AdapterError::Parse("JSON end not found".to_string()))?;
    if end < start {
        return Err(AdapterError::Parse("JSON end before start".to_string()));
    }
    Ok(&payload[start..=end])
}

pub fn extract_xml_region<'a>(payload: &'a [u8], root_tag: &str) -> Result<&'a [u8], AdapterError> {
    let start = find_bytes(payload, b"<?xml")
        .or_else(|| find_bytes(payload, format!("<{root_tag}").as_bytes()))
        .or_else(|| payload.iter().position(|byte| *byte == b'<'))
        .ok_or_else(|| AdapterError::Parse("XML start not found".to_string()))?;
    let closing = format!("</{root_tag}>");
    if let Some(end) = find_bytes_from_end(payload, closing.as_bytes()) {
        return Ok(&payload[start..end + closing.len()]);
    }
    let end = payload
        .iter()
        .rposition(|byte| *byte == b'>')
        .ok_or_else(|| AdapterError::Parse("XML end not found".to_string()))?;
    if end < start {
        return Err(AdapterError::Parse("XML end before start".to_string()));
    }
    Ok(&payload[start..=end])
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn find_bytes_from_end(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .rposition(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aes::Aes128;
    use cbc::cipher::{block_padding::NoPadding, BlockEncryptMut, KeyIvInit};

    #[test]
    fn decrypts_iv_prefixed_payload() {
        let iv = [7_u8; 16];
        let mut plaintext = b"xxxx{\"name\":\"Ada\"}yyyy".to_vec();
        plaintext.resize(32, 0);
        let key = derive_dummy_key();
        let mut buffer = plaintext.clone();
        let ciphertext = cbc::Encryptor::<Aes128>::new_from_slices(&key, &iv)
            .unwrap()
            .encrypt_padded_mut::<NoPadding>(&mut buffer, plaintext.len())
            .unwrap();

        let mut raw = iv.to_vec();
        raw.extend_from_slice(ciphertext);
        let decrypted = decrypt_iv_prefixed_payload(&raw).unwrap();
        let json = extract_json_region(&decrypted).unwrap();

        assert_eq!(json, br#"{"name":"Ada"}"#);
    }
}
