# Persistence

Enable the `persistence` feature to use `PersistencePlugin` with a storage backend.

## Automatic saves

Configure the interval with `autosave(interval)`. Integer arguments are milliseconds; `core::time::Duration` and `std::time::Duration` specify a duration directly.

```rust
use core::time::Duration;
use mirui::core::{persistence::PersistencePlugin, storage::MemoryStorage};

let plugin = PersistencePlugin::new(MemoryStorage::new()).autosave(1000);
let plugin = plugin.autosave(Duration::from_secs(2));
```

Periodic saving is disabled unless configured. Zero disables it again. Negative or unrepresentable integer intervals emit a warning and leave the previous configuration unchanged. Suspend and quit still attempt to save registered data.

Install a clock plugin before the persistence plugin to enable periodic saves. Due saves run at a persistence scheduling boundary, not on a separate timer thread. Submillisecond intervals retain their precision but cannot schedule saves more frequently than those boundaries.

## Storage results

`Storage::read` returns `Result<Option<Vec<u8>>, StorageError>`: only `Ok(None)` means that the key is missing. `write` returns `WriteOutcome::Unchanged` when the bytes match or `WriteOutcome::Written` after the backend accepts them. `remove` also reports failure. A successful result does not guarantee survival across power loss.

`FileStorage::open` and `LocalStorageStorage::with_prefix` return `Result`. A corrupt file is not replaced with an empty store. File writes and removals restore the confirmed in-memory cache if the filesystem operation fails, so retrying cannot mistake uncommitted bytes for a successful save. Use one owner per storage namespace; these backends do not coordinate concurrent writers.

Registry operations return `PersistenceError`. `save_all` attempts all registered items and returns the first failure. Plugin lifecycle hooks log failures and retry saves on subsequent save boundaries. Failed restores do not install a saving callback and do not overwrite the stored record.

Custom byte restore callbacks return `Result<(), PersistenceError>`. Validate the full record before modifying application state; returning an error does not roll back changes made by callback code. Save callbacks must not advance an “already saved” marker while encoding: storage may still reject the write. Identical bytes are deduplicated by the storage backend.
