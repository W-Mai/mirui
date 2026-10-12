use super::{SettingsModel, SettingsModelHandle, build_widgets};
use crate::core::reactive::flush_signal_dirty;
use crate::prelude::*;
use crate::ui::widgets::{Checkbox, Slider};

type TestApp = App<crate::surface::framebuf::FramebufSurface<fn(&[u8], PhysicalRect)>>;

fn mount(app: &mut TestApp, settings: SettingsModelHandle) {
    app.with_default_widgets();
    let root = app.spawn_root().id();
    app.compose(root, build_widgets(settings));
}

fn assert_controls(app: &TestApp, size: u16, details: bool) {
    let world = &app.world;
    let slider = world.find_by_id("settings_size").unwrap();
    let checkbox = world.find_by_id("settings_details").unwrap();
    let title = world.find_by_id("settings_preview_title").unwrap();
    let detail = world.find_by_id("settings_preview_details").unwrap();
    assert_eq!(
        world.get::<Slider>(slider).unwrap().value,
        Fixed::from_int(i32::from(size))
    );
    assert_eq!(world.get::<Checkbox>(checkbox).unwrap().checked, details);
    assert_eq!(world.get::<Style>(title).unwrap().font_size, Some(size));
    assert_eq!(world.has::<crate::ui::Hidden>(detail), !details);
}

#[test]
fn controls_update_the_model_and_preview_without_a_plugin() {
    let mut app = App::headless(320, 320);
    let settings = app.add_model(SettingsModel::default()).finish();
    mount(&mut app, settings.clone());
    assert_controls(&app, 18, true);
    let slider = app.world.find_by_id("settings_size").unwrap();
    crate::input::event::emission::submit_if_listened(
        &mut app.world,
        slider,
        crate::ui::widgets::slider::ValueChanged {
            old: Fixed::from_int(18),
            new: Fixed::from_int(24),
        },
    )
    .unwrap();
    crate::__macro_support::flush_entity_events(&mut app.world).unwrap();
    let checkbox = app.world.find_by_id("settings_details").unwrap();
    crate::__macro_support::dispatch_entity_gesture(
        &mut app.world,
        &crate::input::event::gesture::GestureEvent::Tap {
            target: checkbox,
            x: Fixed::ZERO,
            y: Fixed::ZERO,
        },
        0,
    )
    .unwrap();
    crate::__macro_support::flush_entity_events(&mut app.world).unwrap();
    flush_signal_dirty(&mut app.world);
    assert_eq!(settings.preview_size(), 24);
    assert!(!settings.show_details());
    assert_controls(&app, 24, false);
}

#[cfg(feature = "persistence")]
#[test]
fn archives_initialize_controls_and_restore_without_resetting_session_state() {
    use crate::core::{
        persistence::{FieldCodec, ModelCodec, PersistencePlugin},
        storage::{MemoryStorage, Storage},
    };
    let mut saved = SettingsModel::default();
    saved.set_font_size(22);
    saved.set_show_details(false);
    saved.toggle_preview();
    let mut bytes = alloc::vec::Vec::new();
    FieldCodec::default().encode(&saved, &mut bytes).unwrap();
    let mut storage = MemoryStorage::new();
    storage.write("reader_settings", &bytes).unwrap();
    let mut app = App::headless(320, 320);
    app.add_plugin(PersistencePlugin::new(storage));
    let settings = app.add_model(SettingsModel::default()).finish();
    assert!(settings.preview_open());
    mount(&mut app, settings.clone());
    assert_controls(&app, 22, false);
    settings.set_font_size(14);
    settings.set_show_details(true);
    app.persistence().save(&settings).unwrap();
    settings.set_font_size(24);
    settings.set_show_details(false);
    settings.toggle_preview();
    app.persistence().restore(&settings).unwrap();
    flush_signal_dirty(&mut app.world);
    assert_controls(&app, 14, true);
    assert!(!settings.preview_open());
}
