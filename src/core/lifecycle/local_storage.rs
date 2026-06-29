//! `Storage` backend over the browser's `localStorage`. Values are
//! hex-encoded because `localStorage` only accepts strings; hex doubles
//! the size but never collides with key syntax and pulls in zero new
//! deps. The 5 MB per-origin budget covers all realistic use.

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use super::storage::Storage;

pub struct LocalStorageStorage {
    storage: web_sys::Storage,
    prefix: String,
}

impl LocalStorageStorage {
    /// Open `window.localStorage`, prefixing every key with
    /// `{prefix}:` so the app doesn't trample other scripts on the
    /// same origin. Returns `None` when the host page rejects
    /// localStorage (private mode, sandbox restriction, etc.).
    pub fn with_prefix(prefix: impl Into<String>) -> Option<Self> {
        let window = web_sys::window()?;
        let storage = window.local_storage().ok().flatten()?;
        Some(Self {
            storage,
            prefix: prefix.into(),
        })
    }

    fn full_key(&self, key: &str) -> String {
        format!("{}:{}", self.prefix, key)
    }
}

impl Storage for LocalStorageStorage {
    fn read(&self, key: &str) -> Option<Vec<u8>> {
        let s = self.storage.get_item(&self.full_key(key)).ok().flatten()?;
        hex_decode(&s)
    }
    fn write(&mut self, key: &str, value: &[u8]) {
        let _ = self
            .storage
            .set_item(&self.full_key(key), &hex_encode(value));
    }
    fn remove(&mut self, key: &str) {
        let _ = self.storage.remove_item(&self.full_key(key));
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

    #[allow(dead_code)]
    fn _signature_check(s: &str) {
        let _ = LocalStorageStorage::with_prefix(s);
    }
}
