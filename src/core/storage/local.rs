//! Browser localStorage with namespaced keys and hex-encoded bytes. Available quota and access permissions are controlled by the host.

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use super::{Storage, StorageError, WriteOutcome};

pub struct LocalStorageStorage {
    storage: web_sys::Storage,
    prefix: String,
}

impl LocalStorageStorage {
    /// Open `window.localStorage`, prefixing keys with `{prefix}:`. Unavailable or denied access returns an error.
    pub fn with_prefix(prefix: impl Into<String>) -> Result<Self, StorageError> {
        let window = web_sys::window().ok_or(StorageError::Unavailable)?;
        let storage = window
            .local_storage()
            .map_err(|_| StorageError::Unavailable)?
            .ok_or(StorageError::Unavailable)?;
        Ok(Self {
            storage,
            prefix: prefix.into(),
        })
    }

    fn full_key(&self, key: &str) -> String {
        format!("{}:{}", self.prefix, key)
    }
}

impl Storage for LocalStorageStorage {
    fn read(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        self.storage
            .get_item(&self.full_key(key))
            .map_err(|_| StorageError::Unavailable)?
            .map(|s| hex_decode(&s).ok_or(StorageError::InvalidData))
            .transpose()
    }
    fn write(&mut self, key: &str, value: &[u8]) -> Result<WriteOutcome, StorageError> {
        let full_key = self.full_key(key);
        let stored = self
            .storage
            .get_item(&full_key)
            .map_err(|_| StorageError::Unavailable)?;
        if stored.as_deref().and_then(hex_decode).as_deref() == Some(value) {
            return Ok(WriteOutcome::Unchanged);
        }
        self.storage
            .set_item(&full_key, &hex_encode(value))
            .map_err(|_| StorageError::Unavailable)?;
        Ok(WriteOutcome::Written)
    }
    fn remove(&mut self, key: &str) -> Result<(), StorageError> {
        self.storage
            .remove_item(&self.full_key(key))
            .map_err(|_| StorageError::Unavailable)
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0F) as usize] as char);
    }
    out
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    let bytes = s.as_bytes();
    for chunk in bytes.chunks_exact(2) {
        let hi = hex_nibble(chunk[0])?;
        let lo = hex_nibble(chunk[1])?;
        out.push((hi << 4) | lo);
    }
    Some(out)
}

fn hex_nibble(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip_covers_byte_range() {
        let bytes: Vec<u8> = (0..=255u8).collect();
        let encoded = hex_encode(&bytes);
        let decoded = hex_decode(&encoded).expect("decode");
        assert_eq!(decoded, bytes);
    }

    #[test]
    fn hex_round_trip_empty() {
        assert_eq!(hex_encode(&[]), "");
        assert_eq!(hex_decode("").expect("decode"), Vec::<u8>::new());
    }

    #[test]
    fn hex_decode_rejects_odd_length() {
        assert!(hex_decode("abc").is_none());
    }

    #[test]
    fn hex_decode_rejects_non_hex_char() {
        assert!(hex_decode("zz").is_none());
    }

    #[test]
    fn hex_decode_accepts_uppercase() {
        assert_eq!(
            hex_decode("DEADBEEF").unwrap(),
            vec![0xDE, 0xAD, 0xBE, 0xEF]
        );
    }
}
