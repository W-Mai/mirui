//! Single-file storage using a synced temporary file and rename. Successful writes do not guarantee directory-entry durability across power loss.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::{Storage, StorageError, WriteOutcome, decode_kv, encode_kv};

pub struct FileStorage {
    path: PathBuf,
    cache: BTreeMap<String, Vec<u8>>,
}

impl FileStorage {
    /// Open `path`. Only a missing file starts empty; malformed data and IO failures return errors without modifying the file.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, StorageError> {
        let path = path.into();
        let cache = match fs::read(&path) {
            Ok(bytes) => decode_kv(&bytes).ok_or(StorageError::InvalidData)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(_) => return Err(StorageError::Io),
        };
        Ok(Self { path, cache })
    }

    fn flush(&self) -> Result<(), StorageError> {
        let bytes = encode_kv(&self.cache)?;
        write_atomic(&self.path, &bytes).map_err(|_| StorageError::Io)
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let tmp = temporary_path(path);
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

fn temporary_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".tmp");
    PathBuf::from(name)
}

impl Storage for FileStorage {
    fn read(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        Ok(self.cache.get(key).cloned())
    }
    fn write(&mut self, key: &str, value: &[u8]) -> Result<WriteOutcome, StorageError> {
        if self.cache.get(key).is_some_and(|stored| stored == value) {
            return Ok(WriteOutcome::Unchanged);
        }
        let previous = self.cache.insert(String::from(key), Vec::from(value));
        if let Err(error) = self.flush() {
            if let Some(previous) = previous {
                *self.cache.get_mut(key).expect("inserted storage key") = previous;
            } else {
                self.cache.remove(key);
            }
            return Err(error);
        }
        Ok(WriteOutcome::Written)
    }
    fn remove(&mut self, key: &str) -> Result<(), StorageError> {
        let Some((key, value)) = self.cache.remove_entry(key) else {
            return Ok(());
        };
        if let Err(error) = self.flush() {
            self.cache.insert(key, value);
            return Err(error);
        }
        Ok(())
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
            let mut s = FileStorage::open(&path).unwrap();
            s.write("k1", b"v1").unwrap();
            s.write("k2", b"v2").unwrap();
        }
        let s = FileStorage::open(&path).unwrap();
        assert_eq!(s.read("k1").unwrap().as_deref(), Some(b"v1".as_slice()));
        assert_eq!(s.read("k2").unwrap().as_deref(), Some(b"v2".as_slice()));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn missing_file_starts_empty() {
        let path = tmp_path("missing");
        let s = FileStorage::open(&path).unwrap();
        assert_eq!(s.read("anything"), Ok(None));
    }

    #[test]
    fn corrupt_file_is_rejected_without_overwriting_it() {
        let path = tmp_path("corrupt");
        fs::write(&path, b"this is not valid kv bytes").unwrap();
        assert!(matches!(
            FileStorage::open(&path),
            Err(StorageError::InvalidData)
        ));
        assert_eq!(fs::read(&path).unwrap(), b"this is not valid kv bytes");
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn remove_drops_key_from_disk() {
        let path = tmp_path("remove");
        {
            let mut s = FileStorage::open(&path).unwrap();
            s.write("k", b"v").unwrap();
            s.remove("k").unwrap();
        }
        let s = FileStorage::open(&path).unwrap();
        assert_eq!(s.read("k"), Ok(None));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn failed_writes_and_removals_leave_confirmed_cache_retryable() {
        let path = tmp_path("retry");
        let mut storage = FileStorage::open(&path).unwrap();
        assert_eq!(storage.write("k", b"old"), Ok(WriteOutcome::Written));
        let temporary = temporary_path(&path);
        fs::create_dir(&temporary).unwrap();
        assert_eq!(storage.write("k", b"old"), Ok(WriteOutcome::Unchanged));
        assert_eq!(storage.write("k", b"new"), Err(StorageError::Io));
        assert_eq!(storage.write("new-key", b"new"), Err(StorageError::Io));
        assert_eq!(storage.remove("k"), Err(StorageError::Io));
        assert_eq!(
            storage.read("k").unwrap().as_deref(),
            Some(b"old".as_slice())
        );
        assert_eq!(storage.read("new-key"), Ok(None));
        fs::remove_dir(&temporary).unwrap();
        assert_eq!(storage.write("k", b"new"), Ok(WriteOutcome::Written));
        storage.remove("k").unwrap();
        assert_eq!(FileStorage::open(&path).unwrap().read("k"), Ok(None));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn temporary_path_never_aliases_a_tmp_extension_destination() {
        let path = Path::new("settings.tmp");
        assert_eq!(temporary_path(path), Path::new("settings.tmp.tmp"));
    }
}
