use mirui::core::reactive::{Signal, flush_signal_dirty};
use mirui::render::path::{Path, PathStore};
use mirui::text::TextPath;
use mirui::types::Fixed;
use mirui::ui;
use mirui::ui::dirty::Dirty;
use mirui::ui::widgets::Text;

#[test]
fn dsl_and_builder_install_the_same_text_path_component() {
    let mut app = mirui::app::App::headless(32, 32);
    app.with_default_widgets();
    app.add_resource(PathStore::new(4).unwrap());
    let root = app.spawn_root().id();
    let path = app.paths().insert(Path::new()).unwrap();

    let dsl = app.compose(root, |cx| {
        ui! {
            Text ("dsl", path: path)
        }
    });
    let builder = app.compose(root, |tree| tree.spawn(Text::build("builder").path(path)));

    assert_eq!(
        app.world.get::<TextPath>(dsl),
        app.world.get::<TextPath>(builder)
    );
}

#[test]
fn static_overrides_subscribe_only_to_the_final_path() {
    let mut app = mirui::app::App::headless(32, 32);
    app.with_default_widgets();
    app.add_resource(PathStore::new(4).unwrap());
    let root = app.spawn_root().id();
    let first = app.paths().insert(Path::new()).unwrap();
    let second = app.paths().insert(Path::new()).unwrap();
    let (dsl, builder) = app.compose(root, |cx| {
        let dsl = ui! {
            Text ("dsl", path: first) [
                TextPath::new(second),
            ]
        };
        let builder = cx.spawn((Text::build("builder").path(first), TextPath::new(second)));
        (dsl, builder)
    });
    for text in [dsl, builder] {
        assert_eq!(app.world.get::<TextPath>(text).unwrap().path(), second);
        app.world.remove::<Dirty>(text);
    }
    app.paths().edit(first, |_| {}).unwrap();
    flush_signal_dirty(&mut app.world);
    for text in [dsl, builder] {
        assert!(!app.world.has::<Dirty>(text));
    }
    app.paths().edit(second, |_| {}).unwrap();
    flush_signal_dirty(&mut app.world);
    for text in [dsl, builder] {
        assert!(app.world.has::<Dirty>(text));
    }
}

#[test]
fn configured_dsl_preserves_path_options() {
    let mut app = mirui::app::App::headless(32, 32);
    app.add_resource(PathStore::new(4).unwrap());
    let root = app.spawn_root().id();
    let path = app.paths().insert(Path::new()).unwrap();

    let text = app.compose(root, |cx| {
        ui! {
            Text (
                "configured",
                path: TextPath::new(path)
                    .with_subpath(2)
                    .with_range(Fixed::from_int(8)..Fixed::from_int(72))
                    .with_offset(Fixed::from_int(6))
            )
        }
    });

    let text_path = app.world.get::<TextPath>(text).unwrap();
    assert_eq!(text_path.path(), path);
    assert_eq!(text_path.subpath(), 2);
    assert_eq!(text_path.start(), Fixed::from_int(8));
    assert_eq!(text_path.end(), Some(Fixed::from_int(72)));
    assert_eq!(text_path.offset(), Fixed::from_int(6));
}

#[test]
fn reactive_dsl_replaces_the_path_subscription() {
    let mut app = mirui::app::App::headless(32, 32);
    app.add_resource(PathStore::new(4).unwrap());
    let root = app.spawn_root().id();
    let first = app.paths().insert(Path::new()).unwrap();
    let second = app.paths().insert(Path::new()).unwrap();
    let selected = Signal::new(first);
    let selected_for_dsl = selected.clone();

    let text = app.compose(root, |cx| {
        ui! {
            Text ("reactive", path: $selected_for_dsl)
        }
    });
    assert_eq!(app.world.get::<TextPath>(text).unwrap().path(), first);

    selected.set(second);
    flush_signal_dirty(&mut app.world);
    assert_eq!(app.world.get::<TextPath>(text).unwrap().path(), second);
    app.world.remove::<Dirty>(text);

    app.paths().edit(first, |_| {}).unwrap();
    flush_signal_dirty(&mut app.world);
    assert!(app.world.get::<Dirty>(text).is_none());

    app.paths().edit(second, |_| {}).unwrap();
    flush_signal_dirty(&mut app.world);
    assert!(app.world.get::<Dirty>(text).is_some());
}
