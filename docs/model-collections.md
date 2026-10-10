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

## Capacity

`new()` starts empty with capacity zero. `with_capacity(n)` prepares capacity, while `bounded(n)` also sets a hard limit. `reserve(additional)` prepares at least `len() + additional` positions, including registered member metadata. Removal retains capacity. Ordinary insertion and replacement do not grow storage; explicitly constructing, importing, or reserving a collection may allocate.

Nested collection shapes are prepared from the installed data. Free outer capacity alone does not cover a larger nested collection shape: ordinary insertion or replacement returns `CollectionError::UnpreparedShape` when the prepared slot is insufficient. Business allocations, such as constructing a new `String`, are separate from collection storage.

Map lookup uses binary search. Insertion and removal can move ordered entries; stable retention is linear. The key index and member storage are retained, and capacity-bounded edits and registered item updates reuse their framework storage.

## Membership sets

Declare `selected: model!(owned ModelSet<UserId>)` for selected IDs, enabled options, or tags. `ModelSet::from_iter(values)` moves and deduplicates input into sorted order; `bounded(n)` prepares an empty set with a hard limit. Members are not Models and do not have individual handles. Use a Map when the stored objects need mutable fields or methods.

`contains(&key)`, `len()`, and `is_empty()` observe membership. `try_insert(value)` returns `Ok(true)` for a new value, `Ok(false)` for a duplicate, or `Err((error, value))` when capacity is insufficient. `remove(&key)` returns whether a value was removed; removing an absent value and inserting a duplicate do not publish changes. `clear` and `retain` preserve prepared capacity.

`for_each` borrows each value in sorted order. Its callback cannot mutate the same Model root while values are borrowed. `to_vec()` requires `K: Clone` and creates an explicit owned snapshot; it is not a zero-allocation iteration API. Borrowed lookup supports `String` keys queried with `&str`.
