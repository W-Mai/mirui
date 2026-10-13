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

## Collections

Mark an owned `ModelList`, `ModelMap`, or `ModelSet` field with `#[persist]` to include it in the root archive:

```rust
#[model(persistence)]
#[derive(Default)]
struct TodoItem {
    #[persist]
    #[observe]
    done: bool,
}

#[model]
impl TodoItem {}

#[model(persistence(key = "todo"))]
struct Todo {
    #[persist]
    items: model!(owned ModelList<TodoItem>),
}

#[model]
impl Todo {}
```

Lists preserve item order. Maps encode business keys in key order; sets encode immutable values in value order. Runtime identities, observers, reserved capacity, and effect handlers are not stored. List and Map members require `Default` and a field codec; Map keys and Set values require `Default`, an ordered comparison, and a supported field codec. Missing fields in newly constructed members retain their default values.

Restoration prepares the entire root, including member observation storage, before replacing current data. Exceeding a collection's hard limit or finding duplicate Map keys or Set values rejects the record. Explicit restoration may allocate preparation storage but does not change the configured hard limit. Installation does not allocate framework storage or invoke effect extractors and handlers.

Restoring a collection retires its previous members and creates new identities, even for unchanged keys or values. The collection handle, its observers, and its aggregate effect handler remain installed; callbacks bound to retired members stop executing. Empty stored collections clear membership while retaining prepared capacity. Ordinary reordering and reconciliation continue to preserve matched member identities.

The [State Todo demo](../src/gallery/demos/state_todo/model.rs) saves task titles, completion, order, and the next sample-label index with the default field codec. Its runtime installs storage before Model registration when persistence is enabled. Titles are owning strings without `#[observe]`; rows read them once during composition, while completion remains observable. Restoration replaces member identities and rebuilds keyed rows from the restored titles. Adding tasks constructs business strings; toggling and reordering do not copy them.

## Registering a Model

The [Reader Settings demo](../src/gallery/demos/persistence_settings/model.rs) stores text size and detail visibility. Its controls and preview subscribe to the same Model; preview expansion is observable but is not saved. The [runtime](../src/gallery/demos/persistence_settings/runtime.rs) installs storage before registration and saves every second. Without persistence, the same controls remain usable in memory.

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

The Gallery's Tidal Atlas and Echo Walker codecs encode replay logs into the plugin's reusable output buffer. Their `prepare` methods validate the log and reconstruct temporary game data; `apply` installs that data. The plugin is installed before `add_model(...).finish()`, so UI composition reads the restored game state.

Folding Ark and Twin Beacons use the same registration flow with their fixed-length game archives. Their codecs retain domain validation and checksums; game methods do not maintain a separate persistence revision or invoke storage.

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
let status = app.persistence().status(&settings);
```

Restoration prepares the entire record before applying it, then publishes observed fields, derived values, and watches together. Static owned children retain their identities and subscriptions. Fields not marked `#[persist]` keep their current values under the default codec. Restoration does not invoke effects extractors or deliver business effects; it does not replay actions.

Missing records leave data unchanged. Failed decoding or storage reads leave data unchanged and protect the entry from automatic overwrite. Successful restoration establishes a clean saved baseline; an explicit successful save can also clear protection. Invalid ownership, retired registrations, busy Models, and disabled persistence are rejected. An absent plugin returns `PersistenceError::Unavailable`.

`PersistenceStatus` distinguishes `Unavailable`, `Unregistered`, `Disabled`, `Clean`, `Pending`, and `Failed { error, protected }`. Queries perform no storage IO or allocation. Registration failures, including duplicate keys and missing codecs, remain visible on the returned Model handle. Child Models are not independent archive registrations. `Clean` describes an acknowledged save candidate, not a fresh check of storage contents or durability.

`Failed` reports the latest operation error and whether automatic overwrite is blocked. Ordinary Model changes do not clear failures; acknowledged writes or applied restorations do. Skipping a clean Model or reading a missing record preserves the existing error and restore protection. Disabling persistence hides a registered entry's saved error behind `Disabled` without discarding it; re-enabling retains the error and marks current data for saving.

## Signals, resources, and bytes

Plugin configuration accepts `.signal(key, signal)`, `.resource::<T>(key)`, and `.bytes(key, save, restore)`. Register additional values through `app.persistence()`, `tree.persistence()` inside `App::compose`, or `res!().persistence()` inside `#[compose]`:

```rust,ignore
app.add_resource(NetworkSettings::default());
app.add_plugin(PersistencePlugin::new(storage).autosave(1000));
app.persistence().register_resource::<NetworkSettings>("network")?;
app.persistence().register_signal("counter", counter.clone())?;
app.persistence().register_bytes("cache", save_cache, restore_cache)?;

app.persistence().save(&counter)?;
app.persistence().save("network")?;
```

Registration immediately restores an existing record when IO is enabled. Missing records leave current values unchanged. Restoring a resource inserts it even if that type is absent; saving a resource reads the current value and skips an absent type. Invalid records or duplicate keys reject registration. Resource and Signal values use postcard; byte callbacks own their complete format. These operations do not turn resources into observable Models.

`save(target)` accepts Model handles, Signal instances, resource borrows supplied by `res!`, and non-Model keys. Resource identity includes its App and type; Signal identity includes its source generation. An unrelated value of the same type cannot select a registration. The same Signal can be registered explicitly in multiple Apps. A source registered under multiple keys returns `AmbiguousTarget` without encoding or writing; use `save("specific-key")` to choose. Keys accept static strings, owned `String`, or `Cow<'static, str>`, and cannot select Model archives.

`remove(key)` deletes stored bytes without unregistering the source, resetting current data, or changing a Model's save candidate. A subsequent save can recreate the record. `restore` and `status` remain Model operations; non-Model restoration occurs during registration.

## Deferred operations

Event, system, and timer callbacks record persistence operations through `commands.persistence()`. Commands run in order after callback borrows end and use the Model's data at that time, not a snapshot captured by the callback.

```rust
#[event]
fn save_settings(settings: model!(Settings), commands: commands!()) {
    commands.persistence().save(settings);
}
```

The same facade provides `restore(model)`, `save_all()`, and `set_enabled(bool)`. Requests retain weak instance identities; retiring a Model invalidates its queued requests even if another instance later uses the same storage key. Storage failures emit a warning and skip that request; later commands still run. Recording a request is not confirmation of successful IO. Read `app.persistence().status(&model)` outside the callback to inspect saved state and errors.

Resources use the same shared-borrow save call for both read-only and mutable parameters:

```rust,ignore
#[event]
fn change_network(settings: res!(mut NetworkSettings), commands: commands!()) {
    settings.server.clear();
    commands.persistence().save(&settings);
}

#[event]
fn save_network(settings: res!(NetworkSettings), commands: commands!()) {
    commands.persistence().save(&settings);
}
```

Saving does not mutate or consume the borrowed resource. The command records its source, not the reference or a snapshot. Changes after `save(&settings)` are included when the callback ends. A preceding `insert_resource` command makes the save read the replacement; a preceding removal leaves no value to save. Foreign App resource targets are rejected. Non-Model registration lookup happens at submission, so `register_resource` followed by `save` in the same batch works in that order.

`commands.persistence()` also provides `register_resource`, `register_signal`, `register_bytes`, `remove`, and `autosave`. Resource registration records a static function and key. Signal and byte registration allocate a temporary capture box released on submission or cancellation. Ordinary save-target conversion and recording add no heap allocations; resource/Signal serialization, custom byte callbacks, and storage IO may allocate. Synchronous composition registration must run after other resource borrows have ended.

For an explicit command budget, use `CommandCapacity::new().persistence(count)`. Recording requests uses the existing reusable command storage; encoding, restoring, and storage IO have their own memory costs. Exceeding the callback's command budget discards its entire unsubmitted batch.

## Automatic saves

Configure the interval with `autosave(interval)`. Integer arguments are milliseconds; `core::time::Duration` and `std::time::Duration` specify a duration directly.

```rust
use core::time::Duration;
use mirui::core::{persistence::PersistencePlugin, storage::MemoryStorage};

let plugin = PersistencePlugin::new(MemoryStorage::new()).autosave(1000);
let plugin = plugin.autosave(Duration::from_secs(2));
```

Periodic saving is disabled unless configured. Zero disables it again. Negative or unrepresentable integer intervals emit a warning and leave the previous configuration unchanged. Suspend and quit still attempt to save registered data.

Disabling persistence also suppresses suspend and quit IO. Re-enabling keeps current Model data. Failed saves remain pending and are retried at subsequent save boundaries; a failed periodic attempt does not retry on every intervening frame. Without a clock, suspend and quit saves still run while periodic saves do not.

Install a clock plugin before the persistence plugin to enable periodic saves. Due saves run at a persistence scheduling boundary, not on a separate timer thread. Submillisecond intervals retain their precision but cannot schedule saves more frequently than those boundaries.

## Storage results

`Storage::read` returns `Result<Option<Vec<u8>>, StorageError>`: only `Ok(None)` means that the key is missing. `write` returns `WriteOutcome::Unchanged` when the bytes match or `WriteOutcome::Written` after the backend accepts them. `remove` also reports failure. A successful result does not guarantee survival across power loss.

`FileStorage::open` and `LocalStorageStorage::with_prefix` return `Result`. A corrupt file is not replaced with an empty store. File writes and removals restore the confirmed in-memory cache if the filesystem operation fails, so retrying cannot mistake uncommitted bytes for a successful save. Use one owner per storage namespace; these backends do not coordinate concurrent writers.

Synchronous persistence operations return `PersistenceError`. `save_all` attempts all registered items and returns the first failure. Plugin lifecycle hooks log failures and retry saves on subsequent save boundaries. Failed signal, resource, or byte restores do not install saving callbacks; Model restores retain a protected endpoint. Neither path automatically overwrites a failed record.

`PersistenceError::Capacity` reports failure to prepare encoding or restoration memory; it is distinct from a storage backend failure.

Custom byte restore callbacks return `Result<(), PersistenceError>`. Validate the full record before modifying application state; returning an error does not roll back changes made by callback code. Save callbacks must not advance an “already saved” marker while encoding: storage may still reject the write. Identical bytes are deduplicated by the storage backend.
