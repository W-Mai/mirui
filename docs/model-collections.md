# Model collections

`ModelList<T>` stores an ordered sequence of Models. `ModelMap<K, T>` associates sorted business keys with Models. `ModelSet<K>` tracks membership of immutable values. All live in `mirui::core::model` and can be declared as owned Model fields. Register the parent once; collection changes share the same transaction and observation lifecycle.

## Keyed data

```rust
use mirui::{model, app::App};
use mirui::core::model::{CollectionError, ModelMap};

#[model]
struct Task {
    #[observe]
    done: bool,
}

#[model]
impl Task {
    fn toggle(&mut self) {
        self.done = !self.done;
    }
}

#[model]
struct Tasks {
    items: model!(owned ModelMap<u64, Task>),
}

#[model]
impl Tasks {
    #[observe]
    fn remaining(&self) -> usize {
        self.items.values().filter(|item| !item.done()).count()
    }
}

fn example() -> Result<(), CollectionError> {
    let data = ModelMap::from_entries([
        (7, Task { done: false }),
        (2, Task { done: false }),
    ]).map_err(|(error, _input)| error)?;
    let mut app = App::headless(32, 32);
    let tasks = app.add_model(Tasks { items: data.into() });
    tasks.items().get(&7).unwrap().toggle();
    assert_eq!(tasks.remaining(), 1);
    Ok(())
}
```

`from_entries` accepts an iterator of `(key, data)` pairs, including an owned `BTreeMap` or `HashMap`. It moves the input, sorts by key, and rejects duplicate keys with `(CollectionError::DuplicateKey, input)`. The returned input contains every entry in key order; no item or key needs `Clone`.

`get` and `contains_key` accept borrowed keys: a `ModelMap<String, Item>` can be queried with `&str`. `try_insert(key, data)` returns `(ModelItemId, Option<Item>)`: a new key has no old value, while an existing key returns the replaced data and keeps its identity. Failure returns `(error, key, data)`.

## Iteration and identity

| Operation | Registered result |
| --- | --- |
| List `get(index)` | Optional item instance at the current position |
| Map `get(&key)` | Optional item instance for the business key |
| `get_by_id(id)` | Optional item instance with that collection identity |
| List `iter()` / Map `values()` | Item instances in list order or key order |
| `entries()` | `(ModelItemId, item)` pairs in the same order |
| Map `for_each(callback)` | Borrowed key and item instance for each entry |

Registered `entries`, `iter`, and `values` hold a structural lease until their iterator is dropped. Item methods remain callable, but collection edits return `CollectionError::Iterating`. An ancestor method that could replace the collection is rejected before execution. Map `for_each` borrows the keys during its callback, so that callback cannot mutate any Model in the same root. Owned Map `iter` and `iter_mut` yield borrowed `(key, item)` pairs under ordinary Rust borrowing rules.

Positions and business keys are not item identities. Removing an item invalidates its previous references; reinserting the same key does not revive them. List reordering and same-key Map replacement preserve identity. Declared event, System, and Timer bindings skip execution if a captured member has been retired.

## Binding rows

Use `entries()` with keyed `walk` to keep each row attached to its Model when the collection changes order:

```rust,ignore
#[compose]
fn task_rows(tasks: model!(Tasks)) {
    ui! {
        Column {
            walk ${ tasks.items().entries() } with entry by entry.0 {
                task_row(entry.1)
            }
        }
    };
}
```

`task_row` is a child composition accepting `item: model!(Task)`. Its reactive properties and event callbacks bind to that item. Reordering retains the row entity and those bindings; removing a member removes its row. Inserting a new member constructs a new row, even if it reuses a retired storage slot.

Keyed row tables retain their allocated storage. Pure reordering does not allocate temporary input or row tables; adding rows still incurs normal widget construction costs. Indexed `walk` retains rows by position rather than Model identity, so use stable member keys when items can move or be replaced.

## Capacity

`new()` starts empty with capacity zero. `with_capacity(n)` prepares capacity, while `bounded(n)` also sets a hard limit. `reserve(additional)` prepares at least `len() + additional` positions, including registered member metadata. Removal retains capacity. Ordinary insertion and replacement do not grow storage; explicitly constructing, importing, or reserving a collection may allocate.

Nested collection shapes are prepared from the installed data. Free outer capacity alone does not cover a larger nested collection shape: ordinary insertion or replacement returns `CollectionError::UnpreparedShape` when the prepared slot is insufficient. Business allocations, such as constructing a new `String`, are separate from collection storage.

List and Map capacity reserves member identities, access paths, observation sources, and transaction storage as well as values. Budget nested collections recursively; a vacant prepared slot still retains its metadata. Set stores immutable values without per-member Model metadata. Virtualizing rows reduces widget storage, not the storage of the underlying Models.

Observation storage also depends on installed consumers. Within each reactive graph partition, the dependency bitmap occupies `source_capacity * ceil(consumer_capacity / 64) * 8` bytes, in addition to source records, consumer records, and pending queues. Collection capacity alone therefore does not determine total RAM. Explicit preparation and loading may temporarily hold old and new buffers at once.

Map lookup uses binary search. Insertion and removal can move ordered entries; stable retention is linear. The key index and member storage are retained, and capacity-bounded edits and registered item updates reuse their framework storage.

## Loading list and map data

`items.load(records)` installs an owned iterator of Model data and retires all previous item identities. The collection field, capacity, hard limit, and its observers remain in place. Loading can prepare larger nested collection shapes; it is an explicit allocating operation, not an ordinary capacity-bounded edit. Existing event, System, and Timer bindings to retired items stop executing.

Map `load` accepts `(key, data)` pairs, rejects duplicate keys, and installs members in sorted key order. It retires previous members even when their business keys also appear in the input. Rejected input is returned in original input order, not sorted order; neither keys nor Models need `Clone`.

Preflight failure leaves the previous dataset intact. `CollectionLoadError::error()` reports the reason; `into_input()` yields the staged prefix, the item that stopped loading, and the remaining iterator in original order, without cloning values. A hard-limit failure stops consuming at the first excess item. An active registered iterator rejects loading before any input item is consumed.

```rust,ignore
if let Err(failure) = todo.items().load(records) {
    let reason = failure.error();
    let recovered = failure.into_input();
    handle_rejected_import(reason, recovered);
}
```

Loading inside a parent Model method shares that parent's transaction. An outer `Err` does not undo installed data or revive retired item references; it suppresses that transaction's observations and effects. Iterator or user-code panics follow the existing Model panic contract.

## Merging list data

`items.reconcile(records, key_of, merge)` matches incoming data against existing Models using `K: Ord` business keys. Matching Models keep their identities, observers, and effect handlers. Missing Models are retired, new keys receive new identities, and incoming order becomes list order. Duplicate keys in either dataset reject the batch before the merge closure runs; errors return the input through `CollectionLoadError`.

```rust,ignore
todo.items().reconcile(
    records,
    |item| item.id,
    |current, incoming| {
        current.title = incoming.title;
    },
)?;
```

`merge` receives `&mut Item` and an owned incoming `Item`. The closure decides which business fields to overwrite; omitted fields keep their existing values. Keys are extracted before any merge, so changing a key field does not change the current matching plan. Observed fields and parent derived values publish through the root transaction before effects. Explicit change watches on merged Models are invalidated conservatively because the closure edits raw data without returning a business change token.

Reconciliation stages input and matching indices and may prepare nested storage. It is not a zero-allocation edit. Matching items with unchanged order do not publish a structural change merely because their fields were merged. A merge panic poisons the enclosing registered Model transaction; completed business changes are not rolled back.

## Merging keyed data

`map.reconcile(entries, merge)` uses the Map's own keys; it does not take a separate key extractor. The input is an owned iterator of `(key, data)` pairs. Matching members retain their identities, original stored keys, observations, and effect handlers. New keys create members, absent keys retire members, and traversal remains sorted by key.

```rust,ignore
workspace.users().reconcile(records, |current, incoming| {
    current.name = incoming.name;
})?;
```

The closure receives `&mut Item` and an incoming `Item`, never a mutable index key. Fields omitted from the merge keep their local values. Duplicate input keys and capacity or iteration conflicts reject the batch before any merge; errors return consumed and remaining input in original order. Explicit watches use the same conservative invalidation as List merges. Key indexing stays aligned with completed member changes if a merge panics; this is not rollback or permission to reuse a poisoned registered Model.

## Membership sets

Declare `selected: model!(owned ModelSet<UserId>)` for selected IDs, enabled options, or tags. `ModelSet::from_iter(values)` moves and deduplicates input into sorted order; `bounded(n)` prepares an empty set with a hard limit. Members are not Models and do not have individual handles. Use a Map when the stored objects need mutable fields or methods.

`contains(&key)`, `len()`, and `is_empty()` observe membership. `try_insert(value)` returns `Ok(true)` for a new value, `Ok(false)` for a duplicate, or `Err((error, value))` when capacity is insufficient. `remove(&key)` returns whether a value was removed; removing an absent value and inserting a duplicate do not publish changes. `clear` and `retain` preserve prepared capacity.

`for_each` borrows each value in sorted order. Its callback cannot mutate the same Model root while values are borrowed. `to_vec()` requires `K: Clone` and creates an explicit owned snapshot; it is not a zero-allocation iteration API. Borrowed lookup supports `String` keys queried with `&str`.

`load(values)` installs and deduplicates input, while `reconcile(values)` preserves stored values that compare equal and changes only membership. Reordering the input or repeating existing members does not publish a reconciliation change. Both methods retain capacity and the hard limit; loading publishes the installed dataset.

The hard limit counts distinct members: a set bounded at two accepts `[1, 1, 2, 2]`. The first excess distinct value stops consumption. On failure, `CollectionLoadError::into_input()` returns the complete consumed prefix, including duplicates, followed by the remaining input. Recoverable staging therefore uses memory proportional to consumed input records, not only distinct members; batch import is an explicit allocating operation.
