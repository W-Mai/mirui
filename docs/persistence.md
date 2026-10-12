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

## Registering a Model

Install the plugin before registering persistent Models. `add_model(value)` creates a pending builder; `finish()` performs registration and returns the shared handle. Restoration completes before observation sources are initialized. Discarding a builder performs no registration or storage access; the type's `must_use` warning reminds callers to finish it.

```rust
app.add_plugin(PersistencePlugin::new(storage).autosave(1000));

let settings = app.add_model(Settings {
    volume: 60,
    panel_open: false,
}).finish();

let document = app.add_model(Settings {
    volume: 40,
    panel_open: true,
}).with_key("documents/42").finish();
```

`with_key` is available for Models declared with `persistence`. It overrides the type's key before any storage read. Keys must be nonempty and unique among live registrations, including signal, resource, and byte registrations. A rejected Model registration still returns a usable handle and logs the persistence error without reading or replacing the conflicting record. Models without a key remain unregistered with storage.

Without the plugin, Models behave normally. Installing it later does not scan or restore existing instances. Missing records retain the supplied values and become initial save candidates. Failed reads or invalid archives retain those values but protect the stored record from automatic overwrite; an explicit successful save clears that protection. A live registration holds only a weak Model endpoint, so retiring the Model releases its key.

`PersistencePlugin::enabled(false)` registers Models without performing IO. Re-enabling the registry keeps current values and marks them for saving; it does not restore old data or clear failed-restore protection. Manual IO returns `PersistenceError::Disabled` while disabled.

Registered roots with persistence declarations retain one save-candidate state for the complete owned tree. Mutations mark it without allocating or copying saved fields, including mutations that return an error; reads do not mark it. Models without a persistence declaration use zero-sized persistence state. Candidate tracking is separate from the storage registration and write acknowledgment.

## Model codecs

Configure a codec on the plugin, not on the Model declaration:

```rust
app.add_plugin(
    PersistencePlugin::new(storage)
        .codec(GameArchive)
        .autosave(1000),
);
let game = app.add_model(GameModel::default()).finish();
```

`GameArchive::Model` selects the target type. The plugin selects the codec before reading an instance's record and retains it for that registration. All independent instances of the type share the configured codec; different Apps can use different configurations. Duplicate configurations for one Model type emit a warning and retain the first. Nested Models remain part of their root's archive, without separate codec selection or storage keys.

Without a custom codec, declared fields use the built-in field archive. A persistent root with no declared fields and no custom codec remains unregistered with storage and emits a warning. A custom codec can handle opaque business fields that do not implement field serialization; omit `#[persist]` on those fields.

`ModelCodec` defines a complete archive format for an associated `Model` type. `encode` writes into a reusable byte buffer; `prepare` validates the entire stored record and prepares a restoration without changing the current Model; `apply` consumes that prepared value. Application must not allocate, deliver business effects, or perform additional fallible validation. A custom codec receives and produces complete storage bytes without an added framework prefix. It can access the raw Model's business data, not only fields marked `#[persist]`.

The internal Model save endpoint keeps a weak registration reference, rejects retired, foreign, busy, or poisoned roots, and confirms only the candidate encoded by a successful storage write. An explicit save makes a clean root retryable if encoding or writing fails. A mutation during a write remains pending after that write succeeds. Encoding buffers are reused independently of storage-backend allocations.

## Manual operations

Use `app.persistence()` to save or restore a registered root by its handle. The stored key and selected codec belong to the registration; callers do not repeat them.

```rust
app.persistence().save(&settings)?;
app.persistence().restore(&settings)?;
app.persistence().save_all()?;
app.persistence().set_enabled(false)?;
```

Restoration prepares the entire record before applying it, then publishes observed fields, derived values, and watches together. Static owned children retain their identities and subscriptions. Fields not marked `#[persist]` keep their current values under the default codec. Restoration does not invoke effects extractors or deliver business effects; it does not replay actions.

Missing records leave data unchanged. Failed decoding or storage reads leave data unchanged and protect the entry from automatic overwrite. Successful restoration establishes a clean saved baseline; an explicit successful save can also clear protection. Invalid ownership, retired registrations, busy Models, and disabled persistence are rejected. An absent plugin returns `PersistenceError::Unavailable`.

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

Registry operations return `PersistenceError`. `save_all` attempts all registered items and returns the first failure. Plugin lifecycle hooks log failures and retry saves on subsequent save boundaries. Failed signal, resource, or byte restores do not install saving callbacks; Model restores retain a protected endpoint. Neither path automatically overwrites a failed record.

`PersistenceError::Capacity` reports failure to prepare encoding or restoration memory; it is distinct from a storage backend failure.

Custom byte restore callbacks return `Result<(), PersistenceError>`. Validate the full record before modifying application state; returning an error does not roll back changes made by callback code. Save callbacks must not advance an “already saved” marker while encoding: storage may still reject the write. Identical bytes are deduplicated by the storage backend.
