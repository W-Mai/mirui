use super::*;
use crate::core::persistence::{EncodeOutcome, PersistencePlugin};
use core::cell::Cell;

struct Codec(Rc<Cell<EncodeOutcome>>);

impl ModelCodec for Codec {
    type Model = Settings;
    type Prepared = u8;

    fn encode(
        &self,
        model: &Settings,
        output: &mut Vec<u8>,
    ) -> Result<EncodeOutcome, PersistenceError> {
        if model.value != 0 {
            output.push(model.value);
        }
        Ok(self.0.get())
    }

    fn prepare(&self, _: &Settings, bytes: &[u8]) -> Result<u8, PersistenceError> {
        match bytes {
            [] => Ok(0),
            [value] => Ok(*value),
            _ => Err(PersistenceError::InvalidArchive),
        }
    }

    fn apply(&self, model: &mut Settings, value: u8) {
        model.value = value;
    }
}

#[test]
fn deferred_attempts_preserve_failures_and_restore_protection_until_written() {
    for invalid_archive in [false, true] {
        let store = Store::default();
        if invalid_archive {
            store
                .0
                .borrow_mut()
                .data
                .write("settings", &[1, 2])
                .unwrap();
        }
        let outcome = Rc::new(Cell::new(EncodeOutcome::Encoded));
        let mut app = App::headless(8, 8);
        app.add_plugin(PersistencePlugin::new(store.clone()).codec(Codec(outcome.clone())));
        let model = app.add_model(settings(7)).finish();
        if !invalid_archive {
            store.0.borrow_mut().fail_write = true;
            assert!(app.persistence().save(&model).is_err());
            store.0.borrow_mut().fail_write = false;
        }
        let status = app.persistence().status(&model);
        let writes = store.0.borrow().writes;
        let bytes = store.0.borrow().data.read("settings").unwrap();
        outcome.set(EncodeOutcome::Deferred);
        app.persistence().save(&model).unwrap();
        assert_eq!(app.persistence().status(&model), status);
        assert_eq!(store.0.borrow().writes, writes);
        assert_eq!(store.0.borrow().data.read("settings").unwrap(), bytes);
        outcome.set(EncodeOutcome::Encoded);
        model.set(0);
        app.persistence().save(&model).unwrap();
        assert_eq!(app.persistence().status(&model), PersistenceStatus::Clean);
        assert_eq!(
            store.0.borrow().data.read("settings").unwrap(),
            Some(Vec::new())
        );
    }
}

#[crate::system]
fn request_save(model: model!(Settings), commands: commands!()) {
    commands.persistence().save(model);
}

#[test]
fn deferred_commands_and_lifecycle_saves_retry_only_at_save_boundaries() {
    let store = Store::default();
    let outcome = Rc::new(Cell::new(EncodeOutcome::Deferred));
    let mut app = App::headless(8, 8);
    app.add_plugin(PersistencePlugin::new(store.clone()).codec(Codec(outcome.clone())));
    app.spawn_root().id();
    let model = app.add_model(settings(9)).finish();
    app.add_system(request_save::system(model.clone()));
    assert!(!app.tick());
    assert_eq!(store.0.borrow().writes, 0);
    assert_eq!(app.persistence().status(&model), PersistenceStatus::Pending);
    app.suspend();
    assert_eq!(store.0.borrow().writes, 0);
    assert_eq!(app.persistence().status(&model), PersistenceStatus::Pending);
    app.resume();
    outcome.set(EncodeOutcome::Encoded);
    app.suspend();
    assert_eq!(store.0.borrow().writes, 1);
    assert_eq!(app.persistence().status(&model), PersistenceStatus::Clean);
}

#[test]
fn deferred_autosaves_wait_for_the_next_interval() {
    std::thread_local! { static NOW: Cell<u64> = const { Cell::new(0) }; }
    NOW.set(0);
    let store = Store::default();
    let outcome = Rc::new(Cell::new(EncodeOutcome::Deferred));
    let mut app = App::headless(8, 8);
    app.add_resource(crate::ecs::MonoClock::new(|| NOW.get()));
    app.add_plugin(
        PersistencePlugin::new(store.clone())
            .codec(Codec(outcome.clone()))
            .autosave(1),
    );
    app.spawn_root().id();
    let model = app.add_model(settings(9)).finish();
    NOW.set(1_000_000);
    assert!(!app.tick());
    assert_eq!(store.0.borrow().writes, 0);
    assert_eq!(app.persistence().status(&model), PersistenceStatus::Pending);
    outcome.set(EncodeOutcome::Encoded);
    NOW.set(1_999_999);
    assert!(!app.tick());
    assert_eq!(store.0.borrow().writes, 0);
    NOW.set(2_000_000);
    assert!(!app.tick());
    assert_eq!(store.0.borrow().writes, 1);
    assert_eq!(app.persistence().status(&model), PersistenceStatus::Clean);
}
