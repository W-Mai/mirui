use super::*;
use crate::core::{model::ModelHandle, persistence::PersistencePlugin};

#[test]
fn skipped_save_and_missing_restore_preserve_errors_until_an_actual_receipt() {
    let store = Store::default();
    seed(&store, "settings", 7);
    let mut app = App::headless(16, 16);
    app.add_plugin(PersistencePlugin::new(store.clone()));
    let model = app.add_model(settings(0)).finish();
    assert_eq!(app.persistence().status(&model), PersistenceStatus::Clean);
    assert_eq!(
        model.read(|_| app.persistence().restore(&model)),
        Err(PersistenceError::ModelBusy)
    );
    let failed = PersistenceStatus::Failed {
        error: PersistenceError::ModelBusy,
        protected: false,
    };
    assert_eq!(app.persistence().status(&model), failed);
    app.persistence().save_all().unwrap();
    assert_eq!(store.0.borrow().writes, 0);
    assert_eq!(app.persistence().status(&model), failed);

    store.0.borrow_mut().data.remove("settings").unwrap();
    app.persistence().restore(&model).unwrap();
    assert_eq!(model.value(), 7);
    assert_eq!(app.persistence().status(&model), failed);
    app.persistence().save(&model).unwrap();
    assert_eq!(store.0.borrow().writes, 1);
    assert_eq!(app.persistence().status(&model), PersistenceStatus::Clean);

    model.read(|_| assert!(app.persistence().restore(&model).is_err()));
    app.persistence().restore(&model).unwrap();
    assert_eq!(app.persistence().status(&model), PersistenceStatus::Clean);
    assert_eq!(store.0.borrow().writes, 1);
}
