use super::composition::build_widgets;
use super::model::TodoModel;
use crate::core::reactive::flush_signal_dirty;
use crate::ecs::World;
use crate::input::event::gesture::GestureEvent;
use crate::prelude::*;
use crate::ui::icons::ICON_CHECK;
use crate::ui::widgets::{Icon, Text};
use crate::ui::{Children, Hidden};

type TestApp = App<crate::surface::framebuf::FramebufSurface<fn(&[u8], PhysicalRect)>>;

fn label_text(world: &World, label: Entity) -> alloc::string::String {
    let t = world.get::<Text>(label).expect("label has Text");
    t.resolve(world).into_owned()
}

fn todo_app() -> (TestApp, Entity, [Entity; 3]) {
    let mut app = App::headless(320, 280);
    let todo = app.add_model(TodoModel::default());
    let parent = app.spawn_root().id();
    app.compose(parent, build_widgets(todo));
    let world = &app.world;

    let root = world.get::<Children>(parent).unwrap().0[0];
    let (summary, rows) = {
        let children = &world.get::<Children>(root).unwrap().0;
        (children[0], [children[1], children[2], children[3]])
    };
    (app, summary, rows)
}

fn tap_row(world: &mut World, row: Entity) {
    {
        let decision = crate::__macro_support::dispatch_entity_gesture(
            world,
            &GestureEvent::Tap {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target: row,
            },
            0,
        )
        .unwrap();
        crate::__macro_support::flush_entity_events(world).unwrap();
        assert_eq!(decision, crate::input::event::BubbleControl::Prevent);
    };
    flush_signal_dirty(world);
}

fn row_label(world: &World, row: Entity) -> alloc::string::String {
    let label = world.get::<Children>(row).unwrap().0[0];
    label_text(world, label)
}

fn row_completed(world: &World, row: Entity) -> bool {
    let mark = world.get::<Children>(row).unwrap().0[1];
    let icon = world.get::<Icon>(mark).expect("completion uses an icon");
    assert!(core::ptr::eq(
        icon.path.cmds.as_ptr(),
        ICON_CHECK.path().cmds.as_ptr()
    ));
    !world.has::<Hidden>(mark)
}

#[test]
fn toggling_a_row_updates_remaining_count() {
    let (mut app, summary, rows) = todo_app();
    let world = &mut app.world;
    assert_eq!(label_text(world, summary), "3 REMAINING");
    assert!(!row_completed(world, rows[0]));

    tap_row(world, rows[0]);
    assert_eq!(label_text(world, summary), "2 REMAINING");
    assert!(row_completed(world, rows[0]));

    tap_row(world, rows[0]);
    assert_eq!(label_text(world, summary), "3 REMAINING");
    assert!(!row_completed(world, rows[0]));
}

#[test]
fn row_states_update_independently() {
    let (mut app, summary, rows) = todo_app();
    let world = &mut app.world;
    assert_eq!(row_label(world, rows[0]), "Buy milk");
    assert_eq!(row_label(world, rows[1]), "Write docs");
    assert_eq!(row_label(world, rows[2]), "Ship release");

    tap_row(world, rows[1]);
    assert_eq!(row_label(world, rows[0]), "Buy milk");
    assert_eq!(row_label(world, rows[1]), "Write docs");
    assert_eq!(row_label(world, rows[2]), "Ship release");
    assert!(!row_completed(world, rows[0]));
    assert!(row_completed(world, rows[1]));
    assert!(!row_completed(world, rows[2]));
    assert_eq!(label_text(world, summary), "2 REMAINING");

    tap_row(world, rows[2]);
    assert_eq!(row_label(world, rows[0]), "Buy milk");
    assert_eq!(row_label(world, rows[1]), "Write docs");
    assert_eq!(row_label(world, rows[2]), "Ship release");
    assert!(!row_completed(world, rows[0]));
    assert!(row_completed(world, rows[1]));
    assert!(row_completed(world, rows[2]));
    assert_eq!(label_text(world, summary), "1 REMAINING");
}
