use mirui::model;

#[cfg(feature = "persistence")]
#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;

#[model(persistence(key = "settings"))]
struct Settings {
    #[persist]
    volume: u8,
    #[observe]
    transient: bool,
    #[persist]
    dark_mode: bool,
    #[cfg(any())]
    #[persist]
    missing: UnavailableType,
}

#[model]
impl Settings {
    fn volume(&self) -> u8 {
        self.volume
    }
    fn set_volume(&mut self, value: u8) {
        self.volume = value;
    }
}

#[model(persistence)]
struct Generic<T: 'static> {
    #[persist]
    value: T,
}

#[model]
impl<T: 'static> Generic<T> {}

#[test]
fn declarations_do_not_change_raw_model_layout() {
    assert_eq!(core::mem::size_of::<Settings>(), 3);
    assert_eq!(core::mem::size_of::<Generic<u32>>(), 4);
    let settings = Settings {
        volume: 12,
        transient: true,
        dark_mode: false,
    };
    assert_eq!(settings.volume, 12);
    assert!(settings.transient);
    assert!(!settings.dark_mode);
    let generic = Generic { value: 7_u32 };
    assert_eq!(generic.value, 7);
}

#[cfg(not(feature = "persistence"))]
#[test]
fn disabled_persistence_does_not_require_field_codecs() {
    struct Opaque;
    let value = Generic { value: Opaque };
    assert_eq!(core::mem::size_of_val(&value.value), 0);
    let mut app = mirui::app::App::headless(16, 16);
    let _handle = app.add_model(value).with_key("opaque").finish();
}

#[cfg(feature = "persistence")]
#[path = "model_persistence/enabled"]
mod enabled {
    use super::*;
    use mirui::core::model::PersistModel;

    mod manual;

    struct RawSettings;

    impl mirui::core::persistence::ModelCodec for RawSettings {
        type Model = Settings;
        type Prepared = u8;
        fn encode(&self, model: &Settings, output: &mut Vec<u8>) -> Result<(), PersistenceError> {
            output.push(model.volume);
            Ok(())
        }
        fn prepare(&self, _: &Settings, bytes: &[u8]) -> Result<u8, PersistenceError> {
            match bytes {
                [value] if *value <= 100 => Ok(*value),
                _ => Err(PersistenceError::InvalidArchive),
            }
        }
        fn apply(&self, model: &mut Settings, value: u8) {
            model.volume = value;
        }
    }

    #[test]
    fn plugin_selects_an_external_codec_before_publishing_the_handle() {
        use mirui::core::{
            persistence::PersistencePlugin,
            storage::{MemoryStorage, Storage},
        };
        let mut storage = MemoryStorage::new();
        storage.write("settings", &[82]).unwrap();
        let mut app = mirui::app::App::headless(16, 16);
        app.add_plugin(PersistencePlugin::new(storage).codec(RawSettings));
        let settings = app
            .add_model(Settings {
                volume: 2,
                transient: true,
                dark_mode: false,
            })
            .finish();
        assert!(settings.transient());
        assert_eq!(settings.volume(), 82);
    }

    #[test]
    fn generic_persistent_models_register_without_changing_their_handle_type() {
        let mut app = mirui::app::App::headless(16, 16);
        app.add_plugin(mirui::core::persistence::PersistencePlugin::new(
            mirui::core::storage::MemoryStorage::new(),
        ));
        let _: GenericHandle<u32> = app
            .add_model(Generic { value: 7_u32 })
            .with_key("generic")
            .finish();
    }
    use mirui::core::persistence::{
        PersistenceError,
        archive::{self, Archive},
        value::PersistValue,
    };

    fn save<T: PersistValue>(value: &T) -> Vec<u8> {
        let mut bytes = Vec::new();
        archive::encode(&T::SCHEMA, &mut bytes, |writer| value.encode(writer)).unwrap();
        bytes
    }

    #[model(persistence)]
    struct OldSettings {
        #[persist]
        dark_mode: bool,
        #[persist]
        volume: u8,
    }
    #[model]
    impl OldSettings {}

    #[test]
    fn generated_fields_match_names_and_ignore_declaration_order() {
        assert_eq!(Settings::KEY, Some("settings"));
        assert_eq!(OldSettings::KEY, None);
        let bytes = save(&OldSettings {
            volume: 80,
            dark_mode: true,
        });
        let mut settings = Settings {
            volume: 1,
            dark_mode: false,
            transient: true,
        };
        assert_eq!(
            crate::tracking_allocator::tracked_allocations(|| {
                let prepared = settings
                    .prepare(Archive::parse(&bytes).unwrap().root())
                    .unwrap();
                assert_eq!(settings.volume, 1);
                settings.apply(prepared);
            }),
            0
        );
        assert_eq!(settings.volume, 80);
        assert!(settings.dark_mode);
        assert!(settings.transient);
        assert_eq!(save(&settings), bytes);
    }

    #[test]
    fn generics_and_raw_identifiers_have_static_codecs() {
        #[model(persistence)]
        struct Keyword {
            #[persist]
            r#type: u8,
            #[persist]
            __mirui_marker: bool,
        }
        #[model]
        impl Keyword {}
        let bytes = save(&Generic { value: 42_u32 });
        let mut target = Generic { value: 0_u32 };
        target.apply(
            target
                .prepare(Archive::parse(&bytes).unwrap().root())
                .unwrap(),
        );
        assert_eq!(target.value, 42);
        let bytes = save(&Keyword {
            r#type: 5,
            __mirui_marker: true,
        });
        assert!(
            Archive::parse(&bytes)
                .unwrap()
                .root()
                .field("type")
                .unwrap()
                .is_some()
        );
    }

    #[model(persistence(key = "parent"))]
    struct Parent {
        #[persist]
        child: model!(owned Settings),
    }
    #[model]
    impl Parent {}

    #[test]
    fn owned_fields_prepare_raw_data_without_replacing_local_state() {
        use mirui::core::model::OwnedModel;
        let source = Parent {
            child: Settings {
                volume: 23,
                dark_mode: true,
                transient: false,
            }
            .into(),
        };
        let mut target = Parent {
            child: Settings {
                volume: 1,
                dark_mode: false,
                transient: true,
            }
            .into(),
        };
        let bytes = save(&source);
        let prepared = target
            .prepare(Archive::parse(&bytes).unwrap().root())
            .unwrap();
        assert_eq!(target.child.read(|value| value.volume), 1);
        target.apply(prepared);
        assert_eq!(target.child.read(|value| value.volume), 23);
        assert!(target.child.read(|value| value.transient));
        let mut corrupt = bytes;
        corrupt.pop();
        assert!(matches!(
            Archive::parse(&corrupt),
            Err(PersistenceError::InvalidArchive)
        ));
        assert_eq!(target.child.read(|value| value.volume), 23);
    }
}
