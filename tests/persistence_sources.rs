#![cfg(feature = "persistence")]

use mirui::{
    app::App,
    core::{
        persistence::{PersistenceError, PersistencePlugin},
        reactive::Signal,
        storage::{MemoryStorage, Storage, StorageError, WriteOutcome},
    },
    prelude::*,
};
use serde::{Deserialize, Serialize};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;

#[derive(Clone, Default)]
struct Store {
    data: Rc<RefCell<MemoryStorage>>,
    writes: Rc<Cell<usize>>,
}

impl Storage for Store {
    fn read(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        self.data.borrow().read(key)
    }
    fn write(&mut self, key: &str, bytes: &[u8]) -> Result<WriteOutcome, StorageError> {
        self.writes.set(self.writes.get() + 1);
        self.data.borrow_mut().write(key, bytes)
    }
    fn remove(&mut self, key: &str) -> Result<(), StorageError> {
        self.data.borrow_mut().remove(key)
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Settings {
    server: String,
}

fn settings(value: &str) -> Settings {
    Settings {
        server: value.into(),
    }
}

fn read<T: serde::de::DeserializeOwned>(store: &Store, key: &str) -> T {
    let bytes = store.read(key).unwrap().unwrap();
    postcard::from_bytes(&bytes[1..]).unwrap()
}

#[system]
fn change(settings: res!(mut Settings), commands: commands!()) {
    settings.server.clear();
    commands.persistence().save(&settings);
    settings.server.push_str("latest");
}

#[system]
fn save_readonly(settings: res!(Settings), commands: commands!()) {
    commands.persistence().save(&settings);
}

#[test]
fn mutable_and_shared_parameters_save_without_moving_or_mutably_borrowing_targets() {
    let store = Store::default();
    let mut app = App::headless(8, 8);
    app.add_resource(settings("before"));
    app.add_plugin(PersistencePlugin::new(store.clone()).resource::<Settings>("network"));
    app.add_system(change::system());
    app.add_system(save_readonly::system());
    app.systems.run_all(&mut app.world).unwrap();
    assert_eq!(read::<Settings>(&store, "network"), settings("latest"));
    assert_eq!(store.writes.get(), 2);
}

#[compose]
fn register_and_save() -> Result<(), PersistenceError> {
    res!()
        .persistence()
        .register_resource::<Settings>("network")?;
    res!().persistence().save("network")
}

#[test]
fn composition_registration_restores_then_saves_through_the_same_registry() {
    let store = Store::default();
    let mut first = App::headless(8, 8);
    first.add_resource(settings("stored"));
    first.add_plugin(PersistencePlugin::new(store.clone()));
    let root = first.spawn_root().id();
    first.compose(root, register_and_save()).unwrap();
    let mut second = App::headless(8, 8);
    second.add_plugin(PersistencePlugin::new(store.clone()));
    let root = second.spawn_root().id();
    second
        .compose(root, |tree| {
            tree.persistence().register_resource::<Settings>("network")
        })
        .unwrap();
    assert_eq!(
        second.world.resource::<Settings>(),
        Some(&settings("stored"))
    );
    assert_eq!(
        second
            .persistence()
            .register_resource::<Settings>("network"),
        Err(PersistenceError::DuplicateKey)
    );
}

#[compose]
fn save_in_other<B: Surface, F: RendererFactory<B>>(
    other: &mut App<B, F>,
) -> Result<(), PersistenceError> {
    res!(Settings).with(|settings| other.persistence().save(&settings))
}

#[test]
fn resource_sources_reject_other_apps_and_ambiguous_keys_without_writes() {
    let store = Store::default();
    let mut first = App::headless(8, 8);
    first.add_resource(settings("first"));
    let root = first.spawn_root().id();
    let mut second = App::headless(8, 8);
    second.add_resource(settings("second"));
    second.add_plugin(PersistencePlugin::new(store.clone()).resource::<Settings>("network"));
    assert_eq!(
        first.compose(root, save_in_other(&mut second)),
        Err(PersistenceError::WrongOwner)
    );
    assert_eq!(store.writes.get(), 0);
    second
        .persistence()
        .register_resource::<Settings>("alternate")
        .unwrap();
    second.add_system(save_readonly::system());
    second.systems.run_all(&mut second.world).unwrap();
    assert_eq!(store.writes.get(), 0);
    second
        .persistence()
        .save(String::from("alternate"))
        .unwrap();
    assert_eq!(read::<Settings>(&store, "alternate"), settings("second"));
    assert!(store.read("network").unwrap().is_none());
}

#[test]
fn signals_match_instances_and_allow_explicit_keys_for_multiple_registrations() {
    let store = Store::default();
    let mut app = App::headless(8, 8);
    app.add_plugin(PersistencePlugin::new(store.clone()));
    let signal = Signal::new(7_u32);
    let unrelated = Signal::new(7_u32);
    assert_eq!(
        app.persistence().save(&signal),
        Err(PersistenceError::Unregistered)
    );
    app.persistence()
        .register_signal("a", signal.clone())
        .unwrap();
    assert_eq!(
        app.persistence().save(&unrelated),
        Err(PersistenceError::Unregistered)
    );
    app.persistence().save(&signal).unwrap();
    assert_eq!(read::<u32>(&store, "a"), 7);
    app.persistence()
        .register_signal("b", signal.clone())
        .unwrap();
    signal.set(11);
    assert_eq!(
        tracking_allocator::tracked_allocations(|| {
            assert_eq!(
                app.persistence().save(&signal),
                Err(PersistenceError::AmbiguousTarget)
            );
        }),
        0
    );
    assert_eq!(store.writes.get(), 1);
    app.persistence().save("b").unwrap();
    assert_eq!(read::<u32>(&store, "a"), 7);
    assert_eq!(read::<u32>(&store, "b"), 11);
    let mut other = App::headless(8, 8);
    let other_store = Store::default();
    other.add_plugin(PersistencePlugin::new(other_store.clone()).signal("shared", signal.clone()));
    other.persistence().save(&signal).unwrap();
    assert_eq!(read::<u32>(&other_store, "shared"), 11);
}

#[model(persistence(key = "model"))]
struct Document {
    #[persist]
    value: u32,
}
#[model]
impl Document {}

#[test]
fn keys_cannot_select_models_and_disabling_or_removing_storage_keeps_registration() {
    let store = Store::default();
    let mut app = App::headless(8, 8);
    app.add_plugin(PersistencePlugin::new(store.clone()).signal("counter", Signal::new(3_u32)));
    let model = app.add_model(Document { value: 5 }).finish();
    assert_eq!(
        app.persistence().save("model"),
        Err(PersistenceError::Unregistered)
    );
    app.persistence().save(&model).unwrap();
    app.persistence().set_enabled(false).unwrap();
    assert_eq!(
        app.persistence().save("counter"),
        Err(PersistenceError::Disabled)
    );
    app.persistence().set_enabled(true).unwrap();
    app.persistence().save("counter").unwrap();
    app.persistence().remove("counter").unwrap();
    assert!(store.read("counter").unwrap().is_none());
    app.persistence().save("counter").unwrap();
    assert_eq!(read::<u32>(&store, "counter"), 3);
    app.persistence().autosave(1000).unwrap();
    app.persistence()
        .autosave(core::time::Duration::ZERO)
        .unwrap();
}

#[system]
fn register_replace_save(settings: res!(Settings), commands: commands!()) {
    commands
        .persistence()
        .register_resource::<Settings>("network");
    commands.insert_resource(Settings {
        server: String::from("replacement"),
    });
    commands.persistence().save(&settings);
}

#[system]
fn remove_save(settings: res!(Settings), commands: commands!()) {
    commands.remove_resource::<Settings>();
    commands.persistence().save(&settings);
}

#[test]
fn resource_commands_resolve_registration_and_current_value_in_submission_order() {
    let store = Store::default();
    let mut app = App::headless(8, 8);
    app.add_resource(settings("initial"));
    app.add_plugin(PersistencePlugin::new(store.clone()));
    app.add_system(register_replace_save::system());
    app.add_system(remove_save::system());
    app.systems.run_all(&mut app.world).unwrap();
    assert_eq!(read::<Settings>(&store, "network"), settings("replacement"));
    assert_eq!(store.writes.get(), 1);
    assert!(app.world.resource::<Settings>().is_none());
}
