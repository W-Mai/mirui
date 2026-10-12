use super::{
    manual::{SharedStore, value},
    *,
};
use mirui::{
    app::App,
    core::{
        persistence::{PersistencePlugin, PersistenceStatus},
        storage::{Storage, StorageError, WriteOutcome},
    },
};
use std::{cell::Cell, rc::Rc};

#[derive(Clone, Default)]
struct ObservedStore {
    store: SharedStore,
    fail_write: Rc<Cell<bool>>,
    reads: Rc<Cell<usize>>,
    writes: Rc<Cell<usize>>,
}

impl Storage for ObservedStore {
    fn read(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        self.reads.set(self.reads.get() + 1);
        self.store.read(key)
    }
    fn write(&mut self, key: &str, bytes: &[u8]) -> Result<WriteOutcome, StorageError> {
        self.writes.set(self.writes.get() + 1);
        if self.fail_write.get() {
            return Err(StorageError::Io);
        }
        self.store.write(key, bytes)
    }
    fn remove(&mut self, key: &str) -> Result<(), StorageError> {
        self.store.remove(key)
    }
}

fn failed(error: PersistenceError, protected: bool) -> PersistenceStatus {
    PersistenceStatus::Failed { error, protected }
}

#[test]
fn status_tracks_registration_identity_without_io_or_allocation() {
    let store = ObservedStore::default();
    let mut app = App::headless(16, 16);
    let early = app.add_model(value(1)).finish();
    assert_eq!(
        app.persistence().status(&early),
        PersistenceStatus::Unavailable
    );
    app.add_plugin(PersistencePlugin::new(store.clone()).codec(RawSettings));
    assert_eq!(
        app.persistence().status(&early),
        PersistenceStatus::Unregistered
    );
    let settings = app.add_model(value(2)).finish();
    let duplicate = app.add_model(value(3)).finish();
    let invalid = app.add_model(value(4)).with_key("").finish();
    let missing = app.add_model(Generic { value: 4_u8 }).finish();
    let reads = store.reads.get();
    let writes = store.writes.get();
    let allocations = tracking_allocator::tracked_allocations(|| {
        for _ in 0..32 {
            assert_eq!(
                app.persistence().status(&settings),
                PersistenceStatus::Pending
            );
            assert_eq!(
                app.persistence().status(&duplicate),
                failed(PersistenceError::DuplicateKey, false)
            );
            assert_eq!(
                app.persistence().status(&invalid),
                failed(PersistenceError::InvalidKey, false)
            );
            assert_eq!(
                app.persistence().status(&missing),
                PersistenceStatus::Unregistered
            );
        }
    });
    assert_eq!(allocations, 0);
    assert_eq!((store.reads.get(), store.writes.get()), (reads, writes));
    app.persistence().save(&settings).unwrap();
    assert_eq!(
        app.persistence().status(&settings.clone()),
        PersistenceStatus::Clean
    );
    settings.set_volume(6);
    assert_eq!(
        app.persistence().status(&settings),
        PersistenceStatus::Pending
    );
    app.persistence().set_enabled(false).unwrap();
    assert_eq!(
        app.persistence().status(&settings),
        PersistenceStatus::Disabled
    );
    assert_eq!(
        app.persistence().status(&duplicate),
        failed(PersistenceError::DuplicateKey, false)
    );
    assert_eq!(
        app.persistence().status(&early),
        PersistenceStatus::Unregistered
    );
    let mut foreign = App::headless(16, 16);
    foreign.add_plugin(PersistencePlugin::new(SharedStore::default()).codec(RawSettings));
    assert_eq!(
        foreign.persistence().status(&settings),
        failed(PersistenceError::WrongOwner, false)
    );
    drop(app);
    let _replacement = foreign.add_model(value(50)).finish();
    assert_eq!(
        foreign.persistence().status(&settings),
        PersistenceStatus::Unregistered
    );
}

#[test]
fn failure_state_retains_protection_across_missing_records_and_disabled_io() {
    let mut store = ObservedStore::default();
    store.write("settings", &[200]).unwrap();
    let mut app = App::headless(16, 16);
    app.add_plugin(PersistencePlugin::new(store.clone()).codec(RawSettings));
    let settings = app.add_model(value(20)).finish();
    assert_eq!(
        app.persistence().status(&settings),
        failed(PersistenceError::InvalidArchive, true)
    );
    store.remove("settings").unwrap();
    app.persistence().restore(&settings).unwrap();
    assert_eq!(
        app.persistence().status(&settings),
        failed(PersistenceError::InvalidArchive, true)
    );
    store.fail_write.set(true);
    assert_eq!(
        app.persistence().save(&settings),
        Err(PersistenceError::Storage(StorageError::Io))
    );
    assert_eq!(
        app.persistence().status(&settings),
        failed(PersistenceError::Storage(StorageError::Io), true)
    );
    settings.set_volume(21);
    app.persistence().set_enabled(false).unwrap();
    assert_eq!(
        app.persistence().status(&settings),
        PersistenceStatus::Disabled
    );
    app.persistence().set_enabled(true).unwrap();
    assert_eq!(
        app.persistence().status(&settings),
        failed(PersistenceError::Storage(StorageError::Io), true)
    );
    assert_eq!(
        app.persistence().save_all(),
        Err(PersistenceError::InvalidArchive)
    );
    assert_eq!(
        app.persistence().status(&settings),
        failed(PersistenceError::InvalidArchive, true)
    );
    store.fail_write.set(false);
    app.persistence().save(&settings).unwrap();
    assert_eq!(
        app.persistence().status(&settings),
        PersistenceStatus::Clean
    );
    settings.set_volume(22);
    store.fail_write.set(true);
    assert!(app.persistence().save_all().is_err());
    assert_eq!(
        app.persistence().status(&settings),
        failed(PersistenceError::Storage(StorageError::Io), false)
    );
    store.fail_write.set(false);
    app.persistence().save_all().unwrap();
    assert_eq!(
        app.persistence().status(&settings),
        PersistenceStatus::Clean
    );
    store.write("settings", &[201]).unwrap();
    assert!(app.persistence().restore(&settings).is_err());
    assert_eq!(
        app.persistence().status(&settings),
        failed(PersistenceError::InvalidArchive, true)
    );
    store.write("settings", &[40]).unwrap();
    app.persistence().restore(&settings).unwrap();
    assert_eq!(
        app.persistence().status(&settings),
        PersistenceStatus::Clean
    );
    assert_eq!(settings.volume(), 40);
}

#[model(persistence(key = "family"))]
struct Family {
    #[persist]
    child: model!(owned Settings),
}
#[model]
impl Family {}

#[model(persistence(key = "no-codec"))]
struct NoCodec;
#[model]
impl NoCodec {}

#[test]
fn child_paths_are_not_independent_archives_and_missing_codecs_are_diagnosed() {
    let mut app = App::headless(16, 16);
    app.add_plugin(PersistencePlugin::new(SharedStore::default()));
    let family = app
        .add_model(Family {
            child: value(1).into(),
        })
        .finish();
    assert_eq!(
        app.persistence().status(&family),
        PersistenceStatus::Pending
    );
    assert_eq!(
        app.persistence().status(&family.child()),
        PersistenceStatus::Unregistered
    );
    let no_codec = app.add_model(NoCodec).finish();
    assert_eq!(
        app.persistence().status(&no_codec),
        failed(PersistenceError::MissingCodec, false)
    );
}
