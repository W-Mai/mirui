use super::*;
use crate::core::persistence::PersistencePlugin;
use core::cell::Cell;

struct SettingsCodec {
    prefix: u8,
    encodes: Rc<Cell<usize>>,
}

impl ModelCodec for SettingsCodec {
    type Model = Settings;
    type Prepared = (u8, bool);

    fn encode(&self, model: &Settings, output: &mut Vec<u8>) -> Result<(), PersistenceError> {
        self.encodes.set(self.encodes.get() + 1);
        output.extend_from_slice(&[self.prefix, model.value, u8::from(model.temporary)]);
        Ok(())
    }

    fn prepare(&self, _: &Settings, bytes: &[u8]) -> Result<Self::Prepared, PersistenceError> {
        match bytes {
            [prefix, value, temporary] if *prefix == self.prefix && *temporary <= 1 => {
                Ok((*value, *temporary != 0))
            }
            _ => Err(PersistenceError::InvalidArchive),
        }
    }

    fn apply(&self, model: &mut Settings, (value, temporary): Self::Prepared) {
        model.value = value;
        model.temporary = temporary;
    }
}

fn codec(prefix: u8) -> SettingsCodec {
    SettingsCodec {
        prefix,
        encodes: Rc::default(),
    }
}

#[test]
fn selected_codec_owns_complete_bytes_and_reuses_save_acknowledgments() {
    let store = Store::default();
    store
        .0
        .borrow_mut()
        .data
        .write("settings", &[7, 24, 0])
        .unwrap();
    let codec = codec(7);
    let encodes = codec.encodes.clone();
    let mut app = App::headless(16, 16);
    app.add_plugin(PersistencePlugin::new(store.clone()).codec(codec));
    let value = app.add_model(settings(1)).finish();
    assert_eq!(value.value(), 24);
    assert!(!value.temporary());
    save(&mut app.world, None).unwrap();
    assert_eq!(encodes.get(), 0);
    value.set(45);
    store.0.borrow_mut().fail_write = true;
    assert_eq!(
        save(&mut app.world, None),
        Err(PersistenceError::Storage(StorageError::Io))
    );
    store.0.borrow_mut().fail_write = false;
    save(&mut app.world, None).unwrap();
    save(&mut app.world, None).unwrap();
    assert_eq!(encodes.get(), 2);
    assert_eq!(
        store.0.borrow().data.read("settings").unwrap().unwrap(),
        [7, 45, 0]
    );
}

struct Opaque(u8);

#[crate::model(persistence(key = "opaque"))]
struct Document {
    data: Opaque,
}

#[crate::model]
impl Document {
    fn value(&self) -> u8 {
        self.data.0
    }
}

struct DocumentCodec(u8);
impl ModelCodec for DocumentCodec {
    type Model = Document;
    type Prepared = u8;
    fn encode(&self, model: &Document, output: &mut Vec<u8>) -> Result<(), PersistenceError> {
        output.extend_from_slice(&[self.0, model.data.0]);
        Ok(())
    }
    fn prepare(&self, _: &Document, bytes: &[u8]) -> Result<u8, PersistenceError> {
        match bytes {
            [prefix, value] if *prefix == self.0 => Ok(*value),
            _ => Err(PersistenceError::InvalidArchive),
        }
    }
    fn apply(&self, model: &mut Document, value: u8) {
        model.data.0 = value;
    }
}

#[test]
fn opaque_fields_need_no_field_codec_and_duplicate_model_config_keeps_first() {
    let store = Store::default();
    store.0.borrow_mut().data.write("opaque", &[3, 19]).unwrap();
    let rejected = Rc::new(DocumentCodec(9));
    let mut app = App::headless(16, 16);
    app.add_plugin(
        PersistencePlugin::new(store.clone())
            .codec(DocumentCodec(3))
            .codec(rejected.clone()),
    );
    assert_eq!(Rc::strong_count(&rejected), 1);
    let value = app.add_model(Document { data: Opaque(0) }).finish();
    assert_eq!(value.value(), 19);
    save(&mut app.world, Some("opaque")).unwrap();
    assert_eq!(
        store.0.borrow().data.read("opaque").unwrap().unwrap(),
        [3, 19]
    );
}

#[test]
fn codec_configuration_is_per_app_and_shared_between_independent_roots() {
    let first_store = Store::default();
    let second_store = Store::default();
    let mut first = App::headless(16, 16);
    let mut second = App::headless(16, 16);
    let configured = codec(1);
    let encodes = configured.encodes.clone();
    first.add_plugin(PersistencePlugin::new(first_store.clone()).codec(configured));
    second.add_plugin(PersistencePlugin::new(second_store.clone()).codec(codec(2)));
    let _one = first.add_model(settings(4)).with_key("one").finish();
    let _two = first.add_model(settings(5)).with_key("two").finish();
    let _other = second.add_model(settings(6)).with_key("one").finish();
    save(&mut first.world, None).unwrap();
    save(&mut second.world, None).unwrap();
    assert_eq!(encodes.get(), 2);
    assert_eq!(
        first_store.0.borrow().data.read("one").unwrap().unwrap(),
        [1, 4, 1]
    );
    assert_eq!(
        first_store.0.borrow().data.read("two").unwrap().unwrap(),
        [1, 5, 1]
    );
    assert_eq!(
        second_store.0.borrow().data.read("one").unwrap().unwrap(),
        [2, 6, 1]
    );
}

#[crate::model(persistence(key = "root"))]
struct Parent {
    #[persist]
    child: crate::model!(owned Settings),
}
#[crate::model]
impl Parent {}

#[test]
fn nested_models_keep_the_root_format_without_separate_codec_selection() {
    let store = Store::default();
    let mut app = App::headless(16, 16);
    let codec = codec(77);
    let encodes = codec.encodes.clone();
    app.add_plugin(PersistencePlugin::new(store.clone()).codec(codec));
    let _parent = app
        .add_model(Parent {
            child: settings(8).into(),
        })
        .finish();
    save(&mut app.world, None).unwrap();
    assert_eq!(encodes.get(), 0);
    let state = store.0.borrow();
    assert!(state.data.read("settings").unwrap().is_none());
    let bytes = state.data.read("root").unwrap().unwrap();
    let codec = FieldCodec::<Parent>::default();
    let mut restored = Parent {
        child: settings(0).into(),
    };
    let patch = codec.prepare(&restored, &bytes).unwrap();
    codec.apply(&mut restored, patch);
    assert_eq!(
        crate::core::model::OwnedModel::read(&restored.child, |child| child.value),
        8
    );
}

#[test]
fn custom_restore_failure_remains_protected_until_successful_explicit_save() {
    let store = Store::default();
    store
        .0
        .borrow_mut()
        .data
        .write("settings", &[7, 1, 3])
        .unwrap();
    let mut app = App::headless(16, 16);
    app.add_plugin(PersistencePlugin::new(store.clone()).codec(codec(7)));
    let value = app.add_model(settings(4)).finish();
    assert_eq!(value.value(), 4);
    assert!(value.temporary());
    assert_eq!(
        save(&mut app.world, None),
        Err(PersistenceError::InvalidArchive)
    );
    assert_eq!(store.0.borrow().writes, 0);
    save(&mut app.world, Some("settings")).unwrap();
    assert_eq!(
        store.0.borrow().data.read("settings").unwrap().unwrap(),
        [7, 4, 1]
    );
}
