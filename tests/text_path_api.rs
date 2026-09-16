use mirui::core::reactive::{Signal, flush_signal_dirty};
use mirui::ecs::World;
use mirui::render::path::{Path, PathStore};
use mirui::text::TextPath;
use mirui::types::Fixed;
use mirui::ui;
use mirui::ui::builder::WidgetBuilder;
use mirui::ui::dirty::Dirty;
use mirui::ui::widgets::Text;
use mirui::ui::{IdMap, ViewRegistry};

fn test_world() -> World {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    world.insert_resource(ViewRegistry::default());
    world.insert_resource(PathStore::new(4).unwrap());
    world
}

#[test]
fn dsl_and_builder_install_the_same_text_path_component() {
    let mut world = test_world();
    let root = WidgetBuilder::new(&mut world).id();
    let path = world
        .resource_mut::<PathStore>()
        .unwrap()
        .insert(Path::new())
        .unwrap();

    let dsl = ui! {
        :(
            parent: root
            world: &mut world
        :)

        Text("dsl", path: path)
    };
    let builder = Text::build("builder").path(path).spawn(&mut world);

    assert_eq!(world.get::<TextPath>(dsl), world.get::<TextPath>(builder));
}

#[test]
fn configured_dsl_preserves_path_options() {
    let mut world = test_world();
    let root = WidgetBuilder::new(&mut world).id();
    let path = world
        .resource_mut::<PathStore>()
        .unwrap()
        .insert(Path::new())
        .unwrap();

    let text = ui! {
        :(
            parent: root
            world: &mut world
        :)

        Text(
            "configured",
            path: TextPath::new(path)
                .with_subpath(2)
                .with_range(Fixed::from_int(8)..Fixed::from_int(72))
                .with_offset(Fixed::from_int(6)),
        )
    };

    let text_path = world.get::<TextPath>(text).unwrap();
    assert_eq!(text_path.path(), path);
    assert_eq!(text_path.subpath(), 2);
    assert_eq!(text_path.start(), Fixed::from_int(8));
    assert_eq!(text_path.end(), Some(Fixed::from_int(72)));
    assert_eq!(text_path.offset(), Fixed::from_int(6));
}

#[test]
fn reactive_dsl_replaces_the_path_subscription() {
    let mut world = test_world();
    let root = WidgetBuilder::new(&mut world).id();
    let first = world
        .resource_mut::<PathStore>()
        .unwrap()
        .insert(Path::new())
        .unwrap();
    let second = world
        .resource_mut::<PathStore>()
        .unwrap()
        .insert(Path::new())
        .unwrap();
    let selected = Signal::new(first);
    let selected_for_dsl = selected.clone();

    let text = ui! {
        :(
            parent: root
            world: &mut world
        :)

        Text("reactive", path: $selected_for_dsl)
    };
    assert_eq!(world.get::<TextPath>(text).unwrap().path(), first);

    selected.set(second);
    flush_signal_dirty(&mut world);
    assert_eq!(world.get::<TextPath>(text).unwrap().path(), second);
    world.remove::<Dirty>(text);

    world
        .resource_mut::<PathStore>()
        .unwrap()
        .edit(first, |_| {})
        .unwrap();
    flush_signal_dirty(&mut world);
    assert!(world.get::<Dirty>(text).is_none());

    world
        .resource_mut::<PathStore>()
        .unwrap()
        .edit(second, |_| {})
        .unwrap();
    flush_signal_dirty(&mut world);
    assert!(world.get::<Dirty>(text).is_some());
}
