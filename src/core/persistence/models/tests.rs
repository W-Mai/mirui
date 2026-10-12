use super::*;
use crate::{
    app::App,
    core::storage::{MemoryStorage, StorageError, WriteOutcome},
};
use alloc::rc::Rc;
use core::cell::RefCell;

mod codecs;
mod lifecycle;
mod receipts;

#[crate::model(persistence(key = "settings"))]
struct Settings {
    #[observe]
    #[persist]
    value: u8,
    #[observe]
    temporary: bool,
}
#[crate::model]
impl Settings {
    fn set(&mut self, value: u8) {
        self.value = value;
    }
}

#[derive(Default)]
struct StoreState {
    data: MemoryStorage,
    reads: usize,
    writes: usize,
    fail_read: bool,
    fail_write: bool,
}

#[derive(Clone, Default)]
struct Store(Rc<RefCell<StoreState>>);
impl Storage for Store {
    fn read(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        let mut state = self.0.borrow_mut();
        state.reads += 1;
        if state.fail_read {
            return Err(StorageError::Unavailable);
        }
        state.data.read(key)
    }
    fn write(&mut self, key: &str, bytes: &[u8]) -> Result<WriteOutcome, StorageError> {
        let mut state = self.0.borrow_mut();
        state.writes += 1;
        if state.fail_write {
            return Err(StorageError::Io);
        }
        state.data.write(key, bytes)
    }
    fn remove(&mut self, key: &str) -> Result<(), StorageError> {
        self.0.borrow_mut().data.remove(key)
    }
}

fn settings(value: u8) -> Settings {
    Settings {
        value,
        temporary: true,
    }
}

fn seed(store: &Store, key: &str, value: u8) {
    let mut bytes = Vec::new();
    FieldCodec::default()
        .encode(&settings(value), &mut bytes)
        .unwrap();
    store.0.borrow_mut().data.write(key, &bytes).unwrap();
}

fn stored(store: &Store, key: &str) -> u8 {
    let bytes = store.0.borrow().data.read(key).unwrap().unwrap();
    let codec = FieldCodec::default();
    let mut value = settings(0);
    let prepared = codec.prepare(&value, &bytes).unwrap();
    codec.apply(&mut value, prepared);
    value.value
}

fn save(world: &mut World, key: Option<&str>) -> Result<(), PersistenceError> {
    world
        .with_resource_box::<PersistenceRegistry, _>(|registry, world| match key {
            Some(key) => registry.save(world, key),
            None => registry.save_all(world),
        })
        .unwrap()
}

#[test]
fn finish_restores_once_using_final_key_before_observation() {
    let store = Store::default();
    seed(&store, "settings", 90);
    seed(&store, "document", 23);
    let mut app = App::headless(16, 16);
    app.add_plugin(super::super::PersistencePlugin::new(store.clone()));
    drop(app.add_model(settings(1)).with_key("document"));
    assert_eq!(store.0.borrow().reads, 0);
    assert_eq!(
        app.world
            .query::<model::RegisteredModel<Settings>>()
            .iter()
            .count(),
        0
    );
    let value = app
        .add_model(settings(1))
        .with_key(alloc::string::String::from("document"))
        .finish();
    assert_eq!(value.value(), 23);
    assert!(value.temporary());
    assert_eq!(store.0.borrow().reads, 1);
    save(&mut app.world, None).unwrap();
    assert_eq!(store.0.borrow().writes, 0);
    value.set(24);
    save(&mut app.world, None).unwrap();
    assert_eq!(stored(&store, "document"), 24);
    assert_eq!(stored(&store, "settings"), 90);
    save(&mut app.world, None).unwrap();
    assert_eq!(store.0.borrow().writes, 1);
}

#[test]
fn missing_archive_saves_initial_data_without_repeated_clean_writes() {
    let store = Store::default();
    let mut app = App::headless(16, 16);
    app.add_plugin(super::super::PersistencePlugin::new(store.clone()));
    let value = app.add_model(settings(12)).finish();
    assert_eq!(value.value(), 12);
    save(&mut app.world, None).unwrap();
    assert_eq!(stored(&store, "settings"), 12);
    save(&mut app.world, None).unwrap();
    assert_eq!(store.0.borrow().writes, 1);
}

#[test]
fn model_candidates_flow_through_autosave_and_suspend_hooks() {
    std::thread_local! { static NOW: core::cell::Cell<u64> = const { core::cell::Cell::new(0) }; }
    let store = Store::default();
    let mut app = App::headless(16, 16);
    NOW.set(0);
    app.add_resource(crate::ecs::MonoClock::new(|| NOW.get()));
    app.add_plugin(
        super::super::PersistencePlugin::new(store.clone())
            .autosave(core::time::Duration::from_nanos(1000)),
    );
    app.spawn_root().id();
    let value = app.add_model(settings(1)).finish();
    NOW.set(999);
    app.tick();
    assert_eq!(store.0.borrow().writes, 0);
    NOW.set(1000);
    app.tick();
    assert_eq!(stored(&store, "settings"), 1);
    NOW.set(2000);
    app.tick();
    assert_eq!(store.0.borrow().writes, 1);
    value.set(2);
    app.suspend();
    assert_eq!(stored(&store, "settings"), 2);
}

#[test]
fn absent_or_late_plugin_never_retroactively_registers_existing_models() {
    let store = Store::default();
    seed(&store, "settings", 99);
    let mut app = App::headless(16, 16);
    let old = app.add_model(settings(1)).finish();
    app.add_plugin(super::super::PersistencePlugin::new(store.clone()));
    old.set(2);
    save(&mut app.world, None).unwrap();
    assert_eq!((store.0.borrow().reads, store.0.borrow().writes), (0, 0));
    let new = app.add_model(settings(3)).finish();
    assert_eq!(new.value(), 99);
    assert_eq!(old.value(), 2);
}

#[test]
fn disabled_registration_and_reenable_keep_current_memory() {
    let store = Store::default();
    seed(&store, "settings", 99);
    let mut app = App::headless(16, 16);
    app.add_plugin(super::super::PersistencePlugin::new(store.clone()).enabled(false));
    let value = app.add_model(settings(1)).finish();
    value.set(2);
    assert_eq!(save(&mut app.world, None), Err(PersistenceError::Disabled));
    assert_eq!((store.0.borrow().reads, store.0.borrow().writes), (0, 0));
    app.world
        .resource_mut::<PersistenceRegistry>()
        .unwrap()
        .set_enabled(true);
    save(&mut app.world, None).unwrap();
    assert_eq!(value.value(), 2);
    assert_eq!(stored(&store, "settings"), 2);
    assert_eq!(store.0.borrow().reads, 0);
}

#[test]
fn failed_restore_is_protected_until_an_explicit_successful_save() {
    for read_error in [false, true] {
        let store = Store::default();
        store
            .0
            .borrow_mut()
            .data
            .write("settings", b"broken")
            .unwrap();
        store.0.borrow_mut().fail_read = read_error;
        let mut app = App::headless(16, 16);
        app.add_plugin(super::super::PersistencePlugin::new(store.clone()));
        let value = app.add_model(settings(1)).finish();
        value.set(2);
        assert!(save(&mut app.world, None).is_err());
        assert_eq!(store.0.borrow().writes, 0);
        store.0.borrow_mut().fail_read = false;
        store.0.borrow_mut().fail_write = true;
        assert!(save(&mut app.world, Some("settings")).is_err());
        store.0.borrow_mut().fail_write = false;
        assert!(save(&mut app.world, None).is_err());
        assert_eq!(store.0.borrow().writes, 1);
        assert_eq!(
            store.0.borrow().data.read("settings").unwrap().unwrap(),
            b"broken"
        );
        save(&mut app.world, Some("settings")).unwrap();
        assert_eq!(stored(&store, "settings"), 2);
        value.set(3);
        save(&mut app.world, None).unwrap();
        assert_eq!(stored(&store, "settings"), 3);
    }
}

#[test]
fn conflicting_keys_are_not_read_and_retirement_releases_the_key() {
    use crate::core::model::SharedValue;
    let store = Store::default();
    let mut app = App::headless(16, 16);
    app.add_plugin(super::super::PersistencePlugin::new(store.clone()));
    let first = app.add_model(settings(1)).finish();
    let entity = app
        .world
        .query::<model::RegisteredModel<Settings>>()
        .iter()
        .next()
        .unwrap()
        .0;
    let duplicate = app.add_model(settings(2)).finish();
    assert_eq!(duplicate.value(), 2);
    assert_eq!(store.0.borrow().reads, 1);
    assert_eq!(
        app.world
            .resource_mut::<PersistenceRegistry>()
            .unwrap()
            .bytes("settings", || None, |_| Ok(())),
        Err(PersistenceError::DuplicateKey)
    );
    app.world.despawn(entity);
    assert!(!first.callback_ready());
    let next = app.add_model(settings(3)).finish();
    assert_eq!(next.value(), 3);
    assert_eq!(store.0.borrow().reads, 2);
    save(&mut app.world, None).unwrap();
    assert_eq!(stored(&store, "settings"), 3);
}

#[test]
fn rejected_keys_and_empty_descriptions_keep_models_usable_without_io() {
    #[crate::model(persistence(key = "empty"))]
    struct Empty {
        #[observe]
        value: u8,
    }
    #[crate::model]
    impl Empty {}
    let store = Store::default();
    let mut app = App::headless(16, 16);
    app.add_plugin(super::super::PersistencePlugin::new(store.clone()));
    let empty = app.add_model(Empty { value: 3 }).finish();
    let invalid = app
        .add_model(settings(4))
        .with_key(alloc::string::String::new())
        .finish();
    assert_eq!(empty.value(), 3);
    assert_eq!(invalid.value(), 4);
    save(&mut app.world, None).unwrap();
    assert_eq!((store.0.borrow().reads, store.0.borrow().writes), (0, 0));
}
