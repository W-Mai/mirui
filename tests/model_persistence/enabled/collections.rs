use super::{manual::SharedStore, *};
use mirui::{
    app::App,
    core::{
        model::{ModelHandle, ModelList, ModelMap, ModelSet, OwnedModel},
        persistence::{PersistencePlugin, PersistenceStatus},
        reactive::Computed,
        storage::Storage,
    },
};
use std::{cell::Cell, rc::Rc};

#[model(persistence)]
#[derive(Default)]
struct Leaf {
    #[persist]
    #[observe]
    value: u16,
}
#[model]
impl Leaf {
    fn increment(&mut self) {
        self.value += 1;
    }
}

#[model(persistence)]
struct Item {
    #[persist]
    #[observe]
    value: u16,
    #[persist]
    notes: model!(owned ModelList<Leaf>),
    #[observe]
    transient: u16,
    pending: Option<u16>,
}
impl Default for Item {
    fn default() -> Self {
        Self {
            value: 0,
            notes: ModelList::bounded(3).into(),
            transient: 17,
            pending: None,
        }
    }
}
#[model]
impl Item {
    fn increment(&mut self) {
        self.value += 1;
        self.pending = Some(self.value);
    }
    #[effects]
    fn take_notice(&mut self) -> Option<u16> {
        self.transient += 1;
        self.pending.take()
    }
}

#[model(persistence(key = "collections"))]
struct Collections {
    #[persist]
    items: model!(owned ModelList<Item>),
    #[persist]
    keyed: model!(owned ModelMap<u16, Item>),
    #[persist]
    tags: model!(owned ModelSet<u16>),
    #[observe]
    transient: u16,
}
#[model]
impl Collections {
    #[observe]
    fn total(&self) -> u32 {
        self.items
            .iter()
            .map(|item| u32::from(item.value()))
            .sum::<u32>()
            + self
                .keyed
                .values()
                .map(|item| u32::from(item.value()))
                .sum::<u32>()
            + self.tags.len() as u32
    }
}

fn item(value: u16) -> Item {
    let mut item = Item {
        value,
        transient: 99,
        pending: Some(value),
        ..Item::default()
    };
    item.notes.load([Leaf { value }]).unwrap();
    item
}

fn data(value: u16, tag_limit: usize) -> Collections {
    let mut items = ModelList::bounded(4);
    items.load([item(value)]).unwrap();
    let mut keyed = ModelMap::bounded(4);
    keyed
        .load([(2, item(value + 1)), (1, item(value + 2))])
        .unwrap();
    let mut tags = ModelSet::bounded(tag_limit);
    if tag_limit > 0 {
        tags.load([3, 1]).unwrap();
    }
    Collections {
        items: items.into(),
        keyed: keyed.into(),
        tags: tags.into(),
        transient: 42,
    }
}

struct CheckedCollections;
impl mirui::core::persistence::ModelCodec for CheckedCollections {
    type Model = Collections;
    type Prepared = <Collections as PersistValue>::Prepared;
    fn encode(&self, value: &Collections, output: &mut Vec<u8>) -> Result<(), PersistenceError> {
        archive::encode(&Collections::SCHEMA, output, |writer| value.encode(writer))
    }
    fn prepare(
        &self,
        value: &Collections,
        bytes: &[u8],
    ) -> Result<Self::Prepared, PersistenceError> {
        value.prepare(Archive::parse(bytes)?.root())
    }
    fn apply(&self, value: &mut Collections, prepared: Self::Prepared) {
        assert_eq!(
            crate::tracking_allocator::tracked_allocations(|| value.apply(prepared)),
            0
        );
    }
}

#[test]
fn collection_archives_are_deterministic_and_apply_without_framework_allocation() {
    let source = data(10, 4);
    let bytes = save(&source);
    let mut reordered = data(10, 8);
    reordered
        .keyed
        .load([(1, item(12)), (2, item(11))])
        .unwrap();
    reordered.tags.load([1, 3]).unwrap();
    assert_eq!(save(&reordered), bytes);
    let mut target = data(1, 4);
    let old_id = target.items.entries().next().unwrap().0;
    let prepared = target
        .prepare(Archive::parse(&bytes).unwrap().root())
        .unwrap();
    assert_eq!(target.items.get_by_id(old_id).unwrap().value(), 1);
    assert_eq!(
        crate::tracking_allocator::tracked_allocations(|| target.apply(prepared)),
        0
    );
    assert!(target.items.get_by_id(old_id).is_none());
    assert_eq!(target.items.get(0).unwrap().value(), 10);
    assert_eq!(target.items.get(0).unwrap().transient(), 17);
    assert_eq!(
        target
            .items
            .get(0)
            .unwrap()
            .read(|item| item.notes.get(0).unwrap().value()),
        10
    );
    assert_eq!(target.transient, 42);
    assert_eq!(target.items.capacity(), 4);
    assert_eq!(target.items.limit(), Some(4));
    assert_eq!(save(&target), bytes);
}

#[test]
fn runtime_restore_retires_members_preserves_facades_and_publishes_without_effects() {
    let mut store = SharedStore::default();
    store.write("collections", &save(&data(10, 4))).unwrap();
    let mut app = App::headless(16, 16);
    app.add_plugin(PersistencePlugin::new(store.clone()).codec(CheckedCollections));
    let model = app.add_model(data(1, 4)).finish();
    let items = model.items();
    let keyed = model.keyed();
    let tags = model.tags();
    let old = items.get(0).unwrap();
    let old_child = old.notes().get(0).unwrap();
    let old_keyed = keyed.get(&1).unwrap();
    assert_eq!(old.value(), 10);
    assert_eq!(old.transient(), 17);
    let root = app.spawn_root().id();
    let observed = model.clone();
    let total = app.compose(root, |_| Computed::new(move || observed.total()));
    assert_eq!(total.get(), 35);
    let effects = Rc::new(Cell::new(0));
    let count = effects.clone();
    items
        .on_effect(move |_: &mirui::core::model::ModelItemEffect<u16>| count.set(count.get() + 1))
        .unwrap();
    store.write("collections", &save(&data(20, 4))).unwrap();
    let lease = items.iter();
    assert_eq!(
        app.persistence().restore(&model),
        Err(PersistenceError::ModelBusy)
    );
    drop(lease);
    app.persistence().restore(&model).unwrap();
    assert_eq!(total.get(), 65);
    assert!(!ModelHandle::callback_ready(&old));
    assert!(!ModelHandle::callback_ready(&old_child));
    assert!(!ModelHandle::callback_ready(&old_keyed));
    assert_eq!(keyed.get(&1).unwrap().value(), 22);
    assert_eq!(items.get(0).unwrap().transient(), 17);
    assert!(tags.contains(&3));
    assert_eq!(effects.get(), 0);
    assert_eq!(app.persistence().status(&model), PersistenceStatus::Clean);
    assert_eq!(
        crate::tracking_allocator::tracked_allocations(|| items.get(0).unwrap().increment()),
        0
    );
    assert_eq!(effects.get(), 1);
    assert_eq!(total.get(), 66);
    assert_eq!(app.persistence().status(&model), PersistenceStatus::Pending);
    items.get(0).unwrap().notes().get(0).unwrap().increment();
    assert_eq!(items.get(0).unwrap().notes().get(0).unwrap().value(), 21);
    let current = items.get(0).unwrap();
    let mut empty = data(1, 4);
    empty.items.clear();
    empty.keyed.clear();
    empty.tags.clear();
    store.write("collections", &save(&empty)).unwrap();
    app.persistence().restore(&model).unwrap();
    assert!(!ModelHandle::callback_ready(&current));
    assert!(items.is_empty());
    assert!(keyed.is_empty());
    assert!(tags.is_empty());
    assert_eq!(total.get(), 0);
    let input = item(30);
    assert_eq!(
        crate::tracking_allocator::tracked_allocations(|| assert!(items.try_push(input).is_ok())),
        0
    );
    assert_eq!(total.get(), 30);
}

#[test]
fn a_late_collection_limit_failure_preserves_the_entire_registered_root() {
    let mut store = SharedStore::default();
    let mut app = App::headless(16, 16);
    app.add_plugin(PersistencePlugin::new(store.clone()));
    let model = app.add_model(data(1, 0)).finish();
    let old = model.items().get(0).unwrap();
    let child = old.notes().get(0).unwrap();
    let old_keyed = model.keyed().get(&1).unwrap();
    store.write("collections", &save(&data(20, 4))).unwrap();
    for _ in 0..3 {
        assert_eq!(
            app.persistence().restore(&model),
            Err(PersistenceError::Capacity)
        );
        assert!(ModelHandle::callback_ready(&old));
        assert!(ModelHandle::callback_ready(&child));
        assert!(ModelHandle::callback_ready(&old_keyed));
        assert_eq!(old.value(), 1);
        assert_eq!(old_keyed.value(), 3);
        assert_eq!(model.total(), 6);
    }
    old.increment();
    assert_eq!(old.value(), 2);
    assert_eq!(child.value(), 1);
}

#[test]
fn typed_duplicate_map_keys_and_set_values_reject_the_archive() {
    let mut bytes = Vec::new();
    let target = ModelMap::<u16, Item>::bounded(3);
    archive::encode(&ModelMap::<u16, Item>::SCHEMA, &mut bytes, |writer| {
        writer.count(2)?;
        for _ in 0..2 {
            writer.sized(|writer| 1_u16.encode(writer))?;
            writer.sized(|writer| item(2).encode(writer))?;
        }
        Ok(())
    })
    .unwrap();
    assert!(matches!(
        target.prepare(Archive::parse(&bytes).unwrap().root()),
        Err(PersistenceError::InvalidArchive)
    ));
    let target = ModelSet::<u16>::bounded(3);
    archive::encode(&ModelSet::<u16>::SCHEMA, &mut bytes, |writer| {
        writer.count(2)?;
        writer.sized(|writer| 7_u16.encode(writer))?;
        writer.sized(|writer| 7_u16.encode(writer))
    })
    .unwrap();
    assert!(matches!(
        target.prepare(Archive::parse(&bytes).unwrap().root()),
        Err(PersistenceError::InvalidArchive)
    ));
}

#[test]
fn empty_archives_clear_members_without_discarding_capacity_or_static_state() {
    let mut source = data(5, 4);
    source.items.clear();
    source.keyed.clear();
    source.tags.clear();
    let bytes = save(&source);
    let mut target = data(1, 4);
    target.apply(
        target
            .prepare(Archive::parse(&bytes).unwrap().root())
            .unwrap(),
    );
    assert!(target.items.is_empty());
    assert!(target.keyed.is_empty());
    assert!(target.tags.is_empty());
    assert_eq!(
        (
            target.items.capacity(),
            target.keyed.capacity(),
            target.tags.capacity()
        ),
        (4, 4, 4)
    );
    assert_eq!(target.transient, 42);
}
