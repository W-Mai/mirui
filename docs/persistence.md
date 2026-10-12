# Persistence

Enable the `persistence` feature to use `PersistencePlugin` with a storage backend.

## Model field descriptions

`#[model(persistence)]` declares a field codec. Add `#[persist]` to fields included in the archive; `persistence(key = "settings")` also declares root key metadata. `#[observe]` remains independent of persistence. Static owned Model fields use their child's codec.

```rust
use mirui::model;

#[model(persistence(key = "settings"))]
struct Settings {
    #[persist]
    volume: u8,
    #[observe]
    panel_open: bool,
}

#[model]
impl Settings {}
```

Field codecs match names and archive types, not declaration order. Missing or incompatible fields retain their current values; incompatible types emit a warning. Invalid records are rejected before applying prepared values. Fields without `#[persist]` are not encoded or reset. These declarations do not change the raw Model's memory layout. Disabling mirui's `persistence` feature removes codec implementations and their field trait requirements; the application does not need a same-named feature.

Registered roots with persistence declarations retain one save-candidate state for the complete owned tree. Mutations mark it without allocating or copying saved fields, including mutations that return an error; reads do not mark it. Models without a persistence declaration use zero-sized persistence state. Candidate tracking is separate from the storage registration and write acknowledgment.

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

`PersistenceError::Capacity` reports failure to prepare encoding or restoration memory; it is distinct from a storage backend failure.

Custom byte restore callbacks return `Result<(), PersistenceError>`. Validate the full record before modifying application state; returning an error does not roll back changes made by callback code. Save callbacks must not advance an “already saved” marker while encoding: storage may still reject the write. Identical bytes are deduplicated by the storage backend.
