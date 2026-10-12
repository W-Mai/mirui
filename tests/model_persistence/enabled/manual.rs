use super::*;
use mirui::{
    app::App,
    core::{
        persistence::PersistencePlugin,
        storage::{MemoryStorage, Storage, StorageError, WriteOutcome},
    },
};
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Default)]
pub(super) struct SharedStore(Rc<RefCell<MemoryStorage>>);
impl Storage for SharedStore {
    fn read(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        self.0.borrow().read(key)
    }
    fn write(&mut self, key: &str, value: &[u8]) -> Result<WriteOutcome, StorageError> {
        self.0.borrow_mut().write(key, value)
    }
    fn remove(&mut self, key: &str) -> Result<(), StorageError> {
        self.0.borrow_mut().remove(key)
    }
}

pub(super) fn value(volume: u8) -> Settings {
    Settings {
        volume,
        transient: true,
        dark_mode: false,
    }
}

#[test]
fn application_operations_use_instance_identity_and_respect_plugin_state() {
    let store = SharedStore::default();
    let mut app = App::headless(16, 16);
    let unregistered = app.add_model(value(5)).finish();
    assert_eq!(
        app.persistence().save(&unregistered),
        Err(PersistenceError::Unavailable)
    );
    app.add_plugin(PersistencePlugin::new(store.clone()).codec(RawSettings));
    assert_eq!(
        app.persistence().restore(&unregistered),
        Err(PersistenceError::Unregistered)
    );
    let settings = app.add_model(value(20)).with_key("one").finish();
    let other = app.add_model(value(70)).with_key("two").finish();
    app.persistence().save(&settings).unwrap();
    app.persistence().save(&other).unwrap();
    settings.set_volume(30);
    app.persistence().restore(&settings).unwrap();
    assert_eq!(settings.volume(), 20);
    assert_eq!(other.volume(), 70);
    app.persistence().set_enabled(false).unwrap();
    assert_eq!(
        app.persistence().save(&settings),
        Err(PersistenceError::Disabled)
    );
    assert_eq!(
        app.persistence().restore(&settings),
        Err(PersistenceError::Disabled)
    );
    assert_eq!(
        app.persistence().save_all(),
        Err(PersistenceError::Disabled)
    );
    app.persistence().set_enabled(true).unwrap();
    settings.set_volume(40);
    app.persistence().save_all().unwrap();
    assert_eq!(store.read("one").unwrap().unwrap(), [40]);
    let mut foreign = App::headless(16, 16);
    foreign.add_plugin(PersistencePlugin::new(MemoryStorage::new()).codec(RawSettings));
    assert_eq!(
        foreign.persistence().restore(&settings),
        Err(PersistenceError::WrongOwner)
    );
    drop(app);
    let replacement = foreign.add_model(value(99)).with_key("one").finish();
    assert_eq!(
        foreign.persistence().restore(&settings),
        Err(PersistenceError::Unregistered)
    );
    assert_eq!(replacement.volume(), 99);
}

#[test]
fn invalid_runtime_restore_protects_storage_until_explicit_save_or_successful_restore() {
    let mut store = SharedStore::default();
    let mut app = App::headless(16, 16);
    app.add_plugin(PersistencePlugin::new(store.clone()).codec(RawSettings));
    let settings = app.add_model(value(12)).finish();
    app.persistence().save(&settings).unwrap();
    store.write("settings", &[200]).unwrap();
    assert_eq!(
        app.persistence().restore(&settings),
        Err(PersistenceError::InvalidArchive)
    );
    settings.set_volume(13);
    assert_eq!(
        app.persistence().save_all(),
        Err(PersistenceError::InvalidArchive)
    );
    assert_eq!(store.read("settings").unwrap().unwrap(), [200]);
    app.persistence().save(&settings).unwrap();
    assert_eq!(store.read("settings").unwrap().unwrap(), [13]);
    store.write("settings", &[201]).unwrap();
    assert_eq!(
        app.persistence().restore(&settings),
        Err(PersistenceError::InvalidArchive)
    );
    store.write("settings", &[80]).unwrap();
    app.persistence().restore(&settings).unwrap();
    assert_eq!(settings.volume(), 80);
    app.persistence().save_all().unwrap();
    assert_eq!(store.read("settings").unwrap().unwrap(), [80]);
    store.remove("settings").unwrap();
    settings.set_volume(32);
    app.persistence().restore(&settings).unwrap();
    assert_eq!(settings.volume(), 32);
    app.persistence().save_all().unwrap();
    assert_eq!(store.read("settings").unwrap().unwrap(), [32]);
}
