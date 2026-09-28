use super::composition::build_widgets;
use super::model::TodoModel;
use crate::core::reactive::flush_signal_dirty;
use crate::input::event::GestureHandler;
use crate::input::event::gesture::GestureEvent;
use crate::prelude::*;
use crate::ui::Children;
use crate::ui::IdMap;
use crate::ui::UiScope;
use crate::ui::widgets::Text;

fn label_text(world: &World, label: Entity) -> alloc::string::String {
    let t = world.get::<Text>(label).expect("label has Text");
    t.resolve(world).into_owned()
}

fn todo_world() -> (World, Entity, [Entity; 3]) {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    let (cell, todo) = crate::core::model::register(&mut world, TodoModel::default());
    let registration = world.spawn_empty();
    world.insert(registration, cell);
    let parent = WidgetBuilder::new(&mut world).id();
    let mut cx = UiScope::new(&mut world, parent);
    build_widgets(&mut cx, todo);
    drop(cx);

    let root = world.get::<Children>(parent).unwrap().0[0];
    let (summary, rows) = {
        let children = &world.get::<Children>(root).unwrap().0;
        (children[0], [children[1], children[2], children[3]])
    };
    (world, summary, rows)
}

fn tap_row(world: &mut World, row: Entity) {
    GestureHandler::trigger(
        world,
        row,
        &GestureEvent::Tap {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target: row,
        },
    );
    flush_signal_dirty(world);
}

fn row_label(world: &World, row: Entity) -> alloc::string::String {
    let label = world.get::<Children>(row).unwrap().0[0];
    label_text(world, label)
}

#[test]
fn toggling_a_row_updates_remaining_count() {
    let (mut world, summary, rows) = todo_world();
    assert_eq!(label_text(&world, summary), "3 REMAINING");

    tap_row(&mut world, rows[0]);
    assert_eq!(label_text(&world, summary), "2 REMAINING");

    tap_row(&mut world, rows[0]);
    assert_eq!(label_text(&world, summary), "3 REMAINING");
}

#[test]
fn row_states_update_independently() {
    let (mut world, summary, rows) = todo_world();
    assert_eq!(row_label(&world, rows[0]), "Buy milk");
    assert_eq!(row_label(&world, rows[1]), "Write docs");
    assert_eq!(row_label(&world, rows[2]), "Ship release");

    tap_row(&mut world, rows[1]);
    assert_eq!(row_label(&world, rows[0]), "Buy milk");
    assert_eq!(row_label(&world, rows[1]), "✓  Write docs");
    assert_eq!(row_label(&world, rows[2]), "Ship release");
    assert_eq!(label_text(&world, summary), "2 REMAINING");

    tap_row(&mut world, rows[2]);
    assert_eq!(row_label(&world, rows[0]), "Buy milk");
    assert_eq!(row_label(&world, rows[1]), "✓  Write docs");
    assert_eq!(row_label(&world, rows[2]), "✓  Ship release");
    assert_eq!(label_text(&world, summary), "1 REMAINING");
}
