//! Byte-level key/value storage trait shared by every persistence backend.
//!
//! `Storage` is intentionally `&str` → `&[u8]`: no serialization, no
//! transactions, no async. Backends are free to be in-process maps,
//! files, browser localStorage, or vendor flash drivers. Typed
//! convenience layers (e.g. `core::persistence::PersistencePlugin`) live in
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

/// A storage operation that could not be completed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageError {
    /// The backend is unavailable or access was denied.
    Unavailable,
    /// A filesystem or device operation failed.
    Io,
    /// The stored representation is malformed.
    InvalidData,
    /// A length, allocation, or backend quota was exceeded.
    Capacity,
}

impl core::fmt::Display for StorageError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::Unavailable => "storage unavailable",
            Self::Io => "storage IO failed",
            Self::InvalidData => "invalid stored data",
            Self::Capacity => "storage capacity exceeded",
        })
    }
}

impl core::error::Error for StorageError {}

/// Successful completion of a write operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriteOutcome {
    /// The stored bytes already matched; no write was performed.
    Unchanged,
    /// The backend accepted the new bytes.
    Written,
}

/// Synchronous byte storage with explicit failure and exact write deduplication.
pub trait Storage {
    /// Read a value. Only a missing key returns `Ok(None)`.
    fn read(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError>;
    /// Write differing bytes. Failed writes must remain retryable; success does not promise power-loss durability.
    fn write(&mut self, key: &str, value: &[u8]) -> Result<WriteOutcome, StorageError>;
    /// Remove a key; an already absent key succeeds.
    fn remove(&mut self, key: &str) -> Result<(), StorageError>;

    /// Erase the concrete backend type so a runtime `match` over backends
    /// can return a uniform [`StorageHandle`].
    fn into_handle(self) -> StorageHandle
    where
        Self: Sized + 'static,
    {
        alloc::boxed::Box::new(self)
    }
}

/// Type-erased [`Storage`] for backends chosen at runtime.
pub type StorageHandle = alloc::boxed::Box<dyn Storage>;

impl Storage for StorageHandle {
    fn read(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        (**self).read(key)
    }
    fn write(&mut self, key: &str, value: &[u8]) -> Result<WriteOutcome, StorageError> {
        (**self).write(key, value)
    }
    fn remove(&mut self, key: &str) -> Result<(), StorageError> {
        (**self).remove(key)
    }
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
    fn read(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        Ok(self.data.get(key).cloned())
    }
    fn write(&mut self, key: &str, value: &[u8]) -> Result<WriteOutcome, StorageError> {
        if self.data.get(key).is_some_and(|stored| stored == value) {
            return Ok(WriteOutcome::Unchanged);
        }
        self.data.insert(String::from(key), Vec::from(value));
        Ok(WriteOutcome::Written)
    }
    fn remove(&mut self, key: &str) -> Result<(), StorageError> {
        self.data.remove(key);
        Ok(())
    }
}

/// Binary KV codec the file / NVS backends share.
/// `[u32 LE count] ([u16 LE key_len][key bytes][u32 LE val_len][val bytes])*`
pub fn encode_kv(map: &BTreeMap<String, Vec<u8>>) -> Result<Vec<u8>, StorageError> {
    let count = u32::try_from(map.len()).map_err(|_| StorageError::Capacity)?;
    let mut len = 4_usize;
    for (key, value) in map {
        u16::try_from(key.len()).map_err(|_| StorageError::Capacity)?;
        u32::try_from(value.len()).map_err(|_| StorageError::Capacity)?;
        len = len
            .checked_add(6)
            .and_then(|n| n.checked_add(key.len()))
            .and_then(|n| n.checked_add(value.len()))
            .ok_or(StorageError::Capacity)?;
    }
    let mut out = Vec::new();
    out.try_reserve_exact(len)
        .map_err(|_| StorageError::Capacity)?;
    out.extend_from_slice(&count.to_le_bytes());
    for (k, v) in map {
        out.extend_from_slice(&(k.len() as u16).to_le_bytes());
        out.extend_from_slice(k.as_bytes());
        out.extend_from_slice(&(v.len() as u32).to_le_bytes());
        out.extend_from_slice(v);
    }
    Ok(out)
}

pub fn decode_kv(mut bytes: &[u8]) -> Option<BTreeMap<String, Vec<u8>>> {
    fn take<'a>(bytes: &mut &'a [u8], len: usize) -> Option<&'a [u8]> {
        let part = bytes.get(..len)?;
        *bytes = &bytes[len..];
        Some(part)
    }
    let count_bytes = take(&mut bytes, 4)?.try_into().ok()?;
    let count = u32::from_le_bytes(count_bytes);
    if u64::from(count) > (bytes.len() / 6) as u64 {
        return None;
    }
    let mut map = BTreeMap::new();
    for _ in 0..count {
        let kl_bytes = take(&mut bytes, 2)?.try_into().ok()?;
        let key_len = u16::from_le_bytes(kl_bytes) as usize;
        let key = core::str::from_utf8(take(&mut bytes, key_len)?).ok()?;
        let vl_bytes = take(&mut bytes, 4)?.try_into().ok()?;
        let val_len = usize::try_from(u32::from_le_bytes(vl_bytes)).ok()?;
        let val = take(&mut bytes, val_len)?;
        if map.contains_key(key) {
            return None;
        }
        map.insert(String::from(key), Vec::from(val));
    }
    bytes.is_empty().then_some(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_storage_round_trip() {
        let mut s = MemoryStorage::new();
        assert_eq!(s.read("foo"), Ok(None));
        assert_eq!(s.write("foo", b"bar"), Ok(WriteOutcome::Written));
        let allocations = crate::test_support::tracked_allocations(|| {
            assert_eq!(s.write("foo", b"bar"), Ok(WriteOutcome::Unchanged));
        });
        assert_eq!(allocations, 0);
        assert_eq!(s.read("foo").unwrap().as_deref(), Some(b"bar".as_slice()));
        s.remove("foo").unwrap();
        assert_eq!(s.read("foo"), Ok(None));
    }

    #[test]
    fn kv_codec_round_trip() {
        let mut map = BTreeMap::new();
        map.insert(String::from("alpha"), Vec::from(b"hello".as_slice()));
        map.insert(String::from("β"), Vec::from(b"\x00\x01\x02".as_slice()));
        let bytes = encode_kv(&map).unwrap();
        let back = decode_kv(&bytes).expect("decode");
        assert_eq!(back, map);
    }

    #[test]
    fn kv_codec_empty() {
        let map = BTreeMap::new();
        let bytes = encode_kv(&map).unwrap();
        assert_eq!(decode_kv(&bytes).expect("decode"), map);
    }

    #[test]
    fn kv_codec_rejects_truncated_input() {
        let mut map = BTreeMap::new();
        map.insert(String::from("k"), Vec::from(b"v".as_slice()));
        let mut bytes = encode_kv(&map).unwrap();
        bytes.truncate(bytes.len() - 1);
        assert!(decode_kv(&bytes).is_none());
    }

    #[test]
    fn kv_codec_rejects_ambiguous_records_and_unrepresentable_lengths() {
        let mut map = BTreeMap::new();
        map.insert(String::from("k"), vec![1]);
        let mut bytes = encode_kv(&map).unwrap();
        let record = bytes[4..].to_vec();
        bytes[..4].copy_from_slice(&2_u32.to_le_bytes());
        bytes.extend_from_slice(&record);
        assert!(decode_kv(&bytes).is_none());
        let mut bytes = encode_kv(&map).unwrap();
        bytes.push(0);
        assert!(decode_kv(&bytes).is_none());
        map.insert("x".repeat(u16::MAX as usize + 1), vec![]);
        assert_eq!(encode_kv(&map), Err(StorageError::Capacity));
    }
}
