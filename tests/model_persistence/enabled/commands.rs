use super::{
    manual::{SharedStore, value},
    *,
};
use mirui::{
    app::App,
    core::{
        persistence::{PersistencePlugin, PersistenceStatus},
        storage::Storage,
    },
    event,
    input::event::{contract::Event, lifecycle::Started},
    system,
};

#[mirui::timer(after = 10)]
fn save_from_timer(settings: model!(Settings), commands: commands!()) {
    settings.set_volume(55);
    commands.persistence().save(settings);
}

#[test]
fn timer_commands_share_the_deferred_persistence_path() {
    std::thread_local! { static NOW: std::cell::Cell<u64> = const { std::cell::Cell::new(0) }; }
    let store = SharedStore::default();
    let mut app = App::headless(16, 16);
    app.add_resource(mirui::ecs::MonoClock::new(|| NOW.get()));
    app.add_plugin(PersistencePlugin::new(store.clone()).codec(RawSettings));
    let settings = app.add_model(value(1)).finish();
    app.spawn_root().id();
    app.add_timer(save_from_timer::timer(settings.clone()));
    app.add_system(mirui::core::timer::timer_system::system());
    NOW.set(0);
    app.tick();
    assert!(store.read("settings").unwrap().is_none());
    NOW.set(10_000_000);
    app.tick();
    assert_eq!(store.read("settings").unwrap().unwrap(), [55]);
    assert_eq!(
        app.persistence().status(&settings),
        PersistenceStatus::Clean
    );
}

#[system]
fn save_after_update(settings: model!(Settings), store: res!(SharedStore), commands: commands!()) {
    commands.persistence().save(settings);
    settings.set_volume(23);
    assert!(store.read("settings").unwrap().is_none());
}

#[event]
fn restore_after_callback(settings: model!(Settings), commands: commands!()) {
    settings.set_volume(30);
    commands.persistence().restore(settings);
    assert_eq!(settings.volume(), 30);
}

#[test]
fn system_commands_save_after_role_borrows_end_using_current_data() {
    let store = SharedStore::default();
    let mut app = App::headless(16, 16);
    app.add_resource(store.clone());
    app.add_plugin(PersistencePlugin::new(store.clone()).codec(RawSettings));
    let settings = app.add_model(value(2)).finish();
    app.spawn_root().id();
    app.add_system(save_after_update::system(settings.clone()));
    app.tick();
    assert_eq!(store.read("settings").unwrap().unwrap(), [23]);
    assert_eq!(
        app.persistence().status(&settings),
        PersistenceStatus::Clean
    );
}

#[test]
fn event_commands_restore_with_the_default_callback_budget() {
    let mut store = SharedStore::default();
    store.write("settings", &[40]).unwrap();
    let mut app = App::headless(16, 16);
    app.add_plugin(PersistencePlugin::new(store.clone()).codec(RawSettings));
    let settings = app.add_model(value(2)).finish();
    app.spawn_root().id();
    app.on_event(
        Started::EVENT,
        restore_after_callback::callback(settings.clone()),
    );
    app.tick();
    assert_eq!(settings.volume(), 40);
    assert_eq!(
        app.persistence().status(&settings),
        PersistenceStatus::Clean
    );
}
