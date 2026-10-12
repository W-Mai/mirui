use super::*;
use crate::{
    app::App,
    core::{
        model,
        persistence::{PersistencePlugin, PersistenceStatus},
        storage::{MemoryStorage, Storage, StorageError, WriteOutcome},
    },
    ecs::commands::CommandQueue,
    test_support::tracked_allocations,
};
use alloc::{rc::Rc, vec::Vec};
use core::cell::{Cell, RefCell};

#[crate::model(persistence(key = "state"))]
struct State {
    #[observe]
    #[persist]
    value: u8,
}
#[crate::model]
impl State {
    fn set(&mut self, value: u8) {
        self.value = value;
    }
}

#[derive(Clone, Default)]
struct Store {
    data: Rc<RefCell<MemoryStorage>>,
    writes: Rc<Cell<usize>>,
    fail: Rc<Cell<bool>>,
}
impl Storage for Store {
    fn read(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        self.data.borrow().read(key)
    }
    fn write(&mut self, key: &str, bytes: &[u8]) -> Result<WriteOutcome, StorageError> {
        self.writes.set(self.writes.get() + 1);
        if self.fail.get() {
            return Err(StorageError::Io);
        }
        self.data.borrow_mut().write(key, bytes)
    }
    fn remove(&mut self, key: &str) -> Result<(), StorageError> {
        self.data.borrow_mut().remove(key)
    }
}

#[test]
fn recording_is_deferred_and_reused_queues_do_not_allocate() {
    let store = Store::default();
    let mut app = App::headless(16, 16);
    app.add_plugin(PersistencePlugin::new(store.clone()));
    let state = app.add_model(State { value: 3 }).finish();
    let mut queue = CommandQueue::bounded(CommandCapacity::new().persistence(2)).unwrap();
    let mut batch = queue.begin();
    batch.commands().persistence().save(&state);
    state.set(9);
    assert_eq!(store.writes.get(), 0);
    batch.finish(&mut app.world).unwrap();
    assert_eq!(app.persistence().status(&state), PersistenceStatus::Clean);
    assert_eq!(
        tracked_allocations(|| {
            for _ in 0..8 {
                let mut batch = queue.begin();
                batch.commands().persistence().save(&state);
                batch.commands().persistence().save_all();
                batch.finish(&mut app.world).unwrap();
            }
        }),
        0
    );
}

#[test]
fn capacity_failure_discards_all_requests_without_changing_state() {
    let store = Store::default();
    let mut app = App::headless(16, 16);
    app.add_plugin(PersistencePlugin::new(store.clone()));
    let state = app.add_model(State { value: 3 }).finish();
    let mut queue = CommandQueue::bounded(CommandCapacity::new().persistence(1)).unwrap();
    let mut batch = queue.begin();
    batch.commands().persistence().set_enabled(false);
    batch.commands().persistence().save(&state);
    assert!(batch.finish(&mut app.world).is_err());
    assert_eq!(store.writes.get(), 0);
    assert_eq!(app.persistence().status(&state), PersistenceStatus::Pending);
    let mut batch = queue.begin();
    batch.commands().persistence().save(&state);
    batch.finish(&mut app.world).unwrap();
    assert_eq!(store.writes.get(), 1);
}

#[test]
fn failed_requests_do_not_stop_later_requests_or_erase_saved_errors() {
    let store = Store::default();
    let mut app = App::headless(16, 16);
    app.add_plugin(PersistencePlugin::new(store.clone()));
    let state = app.add_model(State { value: 3 }).finish();
    let mut queue = CommandQueue::bounded(CommandCapacity::new().persistence(4)).unwrap();
    let mut batch = queue.begin();
    batch.commands().persistence().set_enabled(false);
    batch.commands().persistence().save(&state);
    batch.commands().persistence().set_enabled(true);
    batch.commands().persistence().save(&state);
    batch.finish(&mut app.world).unwrap();
    assert_eq!(store.writes.get(), 1);
    store.fail.set(true);
    let mut batch = queue.begin();
    batch.commands().persistence().save(&state);
    batch.commands().persistence().set_enabled(false);
    batch.finish(&mut app.world).unwrap();
    assert_eq!(
        app.persistence().status(&state),
        PersistenceStatus::Disabled
    );
    app.persistence().set_enabled(true).unwrap();
    assert!(matches!(
        app.persistence().status(&state),
        PersistenceStatus::Failed {
            protected: false,
            ..
        }
    ));
}

#[test]
fn pending_requests_do_not_keep_a_retired_root_alive_or_follow_reused_keys() {
    let store = Store::default();
    let mut app = App::headless(16, 16);
    app.add_plugin(PersistencePlugin::new(store.clone()));
    let stale = app.add_model(State { value: 3 }).finish();
    let entity = app
        .world
        .query::<model::RegisteredModel<State>>()
        .iter()
        .next()
        .unwrap()
        .0;
    let mut queue = CommandQueue::bounded(CommandCapacity::new().persistence(3)).unwrap();
    let mut batch = queue.begin();
    batch.commands().persistence().save(&stale);
    batch.commands().persistence().restore(&stale);
    app.world.despawn(entity);
    assert!(stale.access().upgrade().is_none());
    let current = app.add_model(State { value: 90 }).finish();
    batch.commands().persistence().save(&current);
    batch.finish(&mut app.world).unwrap();
    assert_eq!(store.writes.get(), 1);
    assert_eq!(current.value(), 90);
    assert_eq!(
        app.persistence().status(&stale),
        PersistenceStatus::Unregistered
    );
}

#[test]
fn foreign_and_unregistered_targets_do_not_rebind_at_commit() {
    let store = Store::default();
    let mut app = App::headless(16, 16);
    let early = app.add_model(State { value: 5 }).finish();
    let mut foreign = App::headless(16, 16);
    foreign.add_plugin(PersistencePlugin::new(Store::default()));
    let other = foreign.add_model(State { value: 7 }).finish();
    let mut queue = CommandQueue::bounded(CommandCapacity::new().persistence(3)).unwrap();
    let mut batch = queue.begin();
    batch.commands().persistence().save(&early);
    batch.commands().persistence().restore(&other);
    app.add_plugin(PersistencePlugin::new(store.clone()));
    let current = app.add_model(State { value: 10 }).finish();
    batch.commands().persistence().save(&current);
    batch.finish(&mut app.world).unwrap();
    assert_eq!(store.writes.get(), 1);
}
