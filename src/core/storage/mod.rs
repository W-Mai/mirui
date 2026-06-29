//! Byte-level key/value storage trait shared by every persistence backend.
//!
//! `Storage` is intentionally `&str` → `&[u8]`: no serialization, no
//! transactions, no async. Backends are free to be in-process maps,
//! files, browser localStorage, or vendor flash drivers. Typed
//! convenience layers (e.g. `lifecycle::PersistencePlugin`) live in
//! their own modules and just feed encoded bytes through this trait.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

#[cfg(feature = "std")]
mod file;

#[cfg(all(feature = "web-canvas", target_arch = "wasm32"))]
mod local;

#[cfg(feature = "std")]
pub use file::FileStorage;

#[cfg(all(feature = "web-canvas", target_arch = "wasm32"))]
pub use local::LocalStorageStorage;

pub trait Storage {
    fn read(&self, key: &str) -> Option<Vec<u8>>;
    fn write(&mut self, key: &str, value: &[u8]);
    fn remove(&mut self, key: &str);
}

#[derive(Default)]
pub struct MemoryStorage {
    data: BTreeMap<String, Vec<u8>>,
}

impl MemoryStorage {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Storage for MemoryStorage {
    fn read(&self, key: &str) -> Option<Vec<u8>> {
        self.data.get(key).cloned()
    }
    fn write(&mut self, key: &str, value: &[u8]) {
        self.data.insert(String::from(key), Vec::from(value));
    }
    fn remove(&mut self, key: &str) {
        self.data.remove(key);
    }
}

/// Binary KV codec the file / NVS backends share.
/// `[u32 LE count] ([u16 LE key_len][key bytes][u32 LE val_len][val bytes])*`
pub fn encode_kv(map: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(map.len() as u32).to_le_bytes());
    for (k, v) in map {
        out.extend_from_slice(&(k.len() as u16).to_le_bytes());
        out.extend_from_slice(k.as_bytes());
        out.extend_from_slice(&(v.len() as u32).to_le_bytes());
        out.extend_from_slice(v);
    }
    out
}

pub fn decode_kv(bytes: &[u8]) -> Option<BTreeMap<String, Vec<u8>>> {
    let mut cursor = 0usize;
    let count_bytes = bytes.get(cursor..cursor + 4)?.try_into().ok()?;
    cursor += 4;
    let count = u32::from_le_bytes(count_bytes);
    let mut map = BTreeMap::new();
    for _ in 0..count {
        let kl_bytes = bytes.get(cursor..cursor + 2)?.try_into().ok()?;
        cursor += 2;
        let key_len = u16::from_le_bytes(kl_bytes) as usize;
        let key = core::str::from_utf8(bytes.get(cursor..cursor + key_len)?).ok()?;
        cursor += key_len;
        let vl_bytes = bytes.get(cursor..cursor + 4)?.try_into().ok()?;
        cursor += 4;
        let val_len = u32::from_le_bytes(vl_bytes) as usize;
        let val = bytes.get(cursor..cursor + val_len)?;
        cursor += val_len;
        map.insert(String::from(key), Vec::from(val));
    }
    Some(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_storage_round_trip() {
        let mut s = MemoryStorage::new();
        assert_eq!(s.read("foo"), None);
        s.write("foo", b"bar");
        assert_eq!(s.read("foo").as_deref(), Some(b"bar".as_slice()));
        s.remove("foo");
        assert_eq!(s.read("foo"), None);
    }

    #[test]
    fn kv_codec_round_trip() {
        let mut map = BTreeMap::new();
        map.insert(String::from("alpha"), Vec::from(b"hello".as_slice()));
        map.insert(String::from("β"), Vec::from(b"\x00\x01\x02".as_slice()));
        let bytes = encode_kv(&map);
        let back = decode_kv(&bytes).expect("decode");
        assert_eq!(back, map);
    }

    #[test]
    fn kv_codec_empty() {
        let map = BTreeMap::new();
        let bytes = encode_kv(&map);
        assert_eq!(decode_kv(&bytes).expect("decode"), map);
    }

    #[test]
    fn kv_codec_rejects_truncated_input() {
        let mut map = BTreeMap::new();
        map.insert(String::from("k"), Vec::from(b"v".as_slice()));
        let mut bytes = encode_kv(&map);
        bytes.truncate(bytes.len() - 1);
        assert!(decode_kv(&bytes).is_none());
    }
}
