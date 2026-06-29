//! Single-file `Storage` backend. Writes go through `write_atomic`
//! (tmp file + `rename(2)`) so a crash or torn write leaves the on-disk
//! state either fully old or fully new — never partial.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::{Storage, decode_kv, encode_kv};

pub struct FileStorage {
    path: PathBuf,
    cache: BTreeMap<String, Vec<u8>>,
}

impl FileStorage {
    /// Open `path`, loading its contents into memory. Missing or
    /// corrupt files start empty so the app boots in a known-good
    /// state instead of panicking on a stale crash.
    pub fn open(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let cache = fs::read(&path)
            .ok()
            .and_then(|bytes| decode_kv(&bytes))
            .unwrap_or_default();
        Self { path, cache }
    }

    fn flush(&self) -> std::io::Result<()> {
        let bytes = encode_kv(&self.cache);
        write_atomic(&self.path, &bytes)
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let tmp = path.with_extension("tmp");
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

impl Storage for FileStorage {
    fn read(&self, key: &str) -> Option<Vec<u8>> {
        self.cache.get(key).cloned()
    }
    fn write(&mut self, key: &str, value: &[u8]) {
        self.cache.insert(String::from(key), Vec::from(value));
        let _ = self.flush();
    }
    fn remove(&mut self, key: &str) {
        self.cache.remove(key);
        let _ = self.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_path(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("mirui-test-{}-{}.bin", name, std::process::id()));
        let _ = fs::remove_file(&p);
        p
    }

    #[test]
    fn round_trip_persists_across_open() {
        let path = tmp_path("round_trip");
        {
            let mut s = FileStorage::open(&path);
            s.write("k1", b"v1");
            s.write("k2", b"v2");
        }
        let s = FileStorage::open(&path);
        assert_eq!(s.read("k1").as_deref(), Some(b"v1".as_slice()));
        assert_eq!(s.read("k2").as_deref(), Some(b"v2".as_slice()));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn missing_file_starts_empty() {
        let path = tmp_path("missing");
        let s = FileStorage::open(&path);
        assert_eq!(s.read("anything"), None);
    }

    #[test]
    fn corrupt_file_starts_empty_not_panicking() {
        let path = tmp_path("corrupt");
        fs::write(&path, b"this is not valid kv bytes").unwrap();
        let s = FileStorage::open(&path);
        assert_eq!(s.read("anything"), None);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn remove_drops_key_from_disk() {
        let path = tmp_path("remove");
        {
            let mut s = FileStorage::open(&path);
            s.write("k", b"v");
            s.remove("k");
        }
        let s = FileStorage::open(&path);
        assert_eq!(s.read("k"), None);
        let _ = fs::remove_file(&path);
    }
}
