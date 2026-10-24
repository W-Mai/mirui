use crate::app::App;
use crate::core::model::ModelList;
use crate::core::reactive::{Signal, flush_signal_dirty};
use crate::ecs::Entity;
use crate::types::Dimension;
use crate::ui::{Children, Parent, Style};

#[crate::model]
struct Item {
    #[observe]
    value: u8,
}

#[crate::model]
impl Item {
    fn increment(&mut self) {
        self.value += 1;
    }
}

#[crate::compose]
fn row(item: model!(Item)) -> Entity {
    crate::ui! { View(width: ${Dimension::px(i32::from(item.value()))}, height: 8) }
}

#[crate::compose]
fn rows(items: model!(ModelList<Item>), pulse: Signal<u8>) -> Entity {
    crate::ui! {
        Column() {
            View(id: "prefix")
            walk ${ let _ = pulse.get(); items.entries() } with entry by entry.0 {
                row(entry.1)
            }
        }
    }
}

#[test]
fn unchanged_keyed_children_preserve_revisions_bindings_and_foreign_siblings() {
    let mut app = App::headless(64, 64);
    let root = app.spawn_root().id();
    let items = app
        .add_model(ModelList::from([Item { value: 1 }, Item { value: 2 }]))
        .finish();
    let pulse = Signal::new(0_u8);
    let list = app.compose(root, rows(items.clone(), pulse.clone()));
    let original = app.world.get::<Children>(list).unwrap().0.clone();
    let external = app.world.spawn_empty();
    app.world.insert(external, Parent(list));
    app.world
        .get_mut::<Children>(list)
        .unwrap()
        .0
        .insert(1, external);
    app.world.watch_component_revision::<Children>();
    let revision = app.world.component_revision::<Children>();
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            for value in 1..=16 {
                pulse.set(value);
                flush_signal_dirty(&mut app.world);
            }
        }),
        0
    );
    assert_eq!(app.world.component_revision::<Children>(), revision);
    assert_eq!(
        app.world.get::<Children>(list).unwrap().0,
        [original[0], external, original[1], original[2]]
    );
    items.get(0).unwrap().increment();
    flush_signal_dirty(&mut app.world);
    assert_eq!(
        app.world.get::<Style>(original[1]).unwrap().layout.width,
        Dimension::px(2)
    );
    assert_eq!(app.world.component_revision::<Children>(), revision);

    items.rotate_left(1).unwrap();
    flush_signal_dirty(&mut app.world);
    assert_ne!(app.world.component_revision::<Children>(), revision);
    assert_eq!(
        app.world.get::<Children>(list).unwrap().0,
        [original[0], external, original[2], original[1]]
    );
    items.clear().unwrap();
    flush_signal_dirty(&mut app.world);
    assert_eq!(
        app.world.get::<Children>(list).unwrap().0,
        [original[0], external]
    );
    assert!(!app.world.is_alive(original[1]));
    assert!(!app.world.is_alive(original[2]));
    let revision = app.world.component_revision::<Children>();
    pulse.set(17);
    flush_signal_dirty(&mut app.world);
    assert_eq!(app.world.component_revision::<Children>(), revision);
}
