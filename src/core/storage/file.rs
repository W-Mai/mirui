//! Single-file storage that writes a temporary file and renames it over the
//! store. The rename is synchronous, so a returned success means the next
//! open reads the new bytes and a failure leaves the store untouched. Syncing
//! the file to the device runs on a background thread, because an `fsync` can
//! stall a frame for tens or hundreds of milliseconds; dropping the storage
//! waits for that sync. Successful writes do not guarantee durability across
//! power loss.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;

use super::{Storage, StorageError, WriteOutcome, decode_kv, encode_kv};

pub struct FileStorage {
    path: PathBuf,
    cache: BTreeMap<String, Vec<u8>>,
    syncer: Option<Syncer>,
}

/// Background thread that syncs the renamed store to the device. Requests
/// that arrive while a sync runs are folded into the next one.
struct Syncer {
    requests: Option<mpsc::Sender<()>>,
    worker: Option<thread::JoinHandle<()>>,
}

impl Syncer {
    fn spawn(path: PathBuf) -> Option<Self> {
        let (requests, pending) = mpsc::channel::<()>();
        let worker = thread::Builder::new()
            .name("mirui-file-storage".into())
            .spawn(move || {
                while pending.recv().is_ok() {
                    while pending.try_recv().is_ok() {}
                    let _ = sync_path(&path);
                }
            })
            .ok()?;
        Some(Self {
            requests: Some(requests),
            worker: Some(worker),
        })
    }

    fn request(&self) -> bool {
        self.requests
            .as_ref()
            .is_some_and(|requests| requests.send(()).is_ok())
    }
}

impl Drop for Syncer {
    fn drop(&mut self) {
        drop(self.requests.take());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Sync the store and, on Unix, the directory entry that names it.
fn sync_path(path: &Path) -> std::io::Result<()> {
    fs::File::open(path)?.sync_all()?;
    #[cfg(unix)]
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::File::open(parent)?.sync_all()?;
    }
    Ok(())
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
        Ok(Self {
            path,
            cache,
            syncer: None,
        })
    }

    fn flush(&mut self) -> Result<(), StorageError> {
        let bytes = encode_kv(&self.cache)?;
        write_atomic(&self.path, &bytes).map_err(|_| StorageError::Io)?;
        if self.syncer.is_none() {
            self.syncer = Syncer::spawn(self.path.clone());
        }
        // Without a worker thread, sync here as before.
        if !self.syncer.as_ref().is_some_and(Syncer::request) {
            sync_path(&self.path).map_err(|_| StorageError::Io)?;
        }
        Ok(())
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let tmp = temporary_path(path);
    fs::File::create(&tmp)?.write_all(bytes)?;
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
    fn writes_return_before_the_device_sync_and_drop_waits_for_it() {
        let path = tmp_path("background_sync");
        let mut storage = FileStorage::open(&path).unwrap();
        for round in 0..20_u8 {
            storage.write("k", &[round]).unwrap();
            // The rename already happened, so a fresh open sees the bytes.
            assert_eq!(
                FileStorage::open(&path).unwrap().read("k").unwrap(),
                Some(alloc::vec![round])
            );
        }
        assert!(storage.syncer.is_some(), "one worker serves every write");
        drop(storage);
        assert_eq!(
            FileStorage::open(&path).unwrap().read("k").unwrap(),
            Some(alloc::vec![19])
        );
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn temporary_path_never_aliases_a_tmp_extension_destination() {
        let path = Path::new("settings.tmp");
        assert_eq!(temporary_path(path), Path::new("settings.tmp.tmp"));
    }
}
