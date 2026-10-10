use super::composition::build_widgets;
use super::model::TodoModel;
use crate::core::reactive::flush_signal_dirty;
use crate::ecs::World;
use crate::input::event::gesture::GestureEvent;
use crate::prelude::*;
use crate::ui::icons::ICON_CHECK;
use crate::ui::widgets::{Icon, Text};
use crate::ui::{Children, Hidden, Parent};

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
        let rows = &world.get::<Children>(children[2]).unwrap().0;
        (children[0], [rows[0], rows[1], rows[2]])
    };
    (app, summary, rows)
}

fn list_and_controls(world: &World, summary: Entity) -> (Entity, [Entity; 3]) {
    let root = world.get::<Parent>(summary).unwrap().0;
    let children = &world.get::<Children>(root).unwrap().0;
    let controls = &world.get::<Children>(children[1]).unwrap().0;
    (children[2], [controls[0], controls[1], controls[2]])
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

#[test]
fn collection_controls_preserve_row_identity_and_remove_completed_members() {
    let (mut app, summary, original) = todo_app();
    let world = &mut app.world;
    let (list, [add, rotate, clear]) = list_and_controls(world, summary);

    tap_row(world, original[0]);
    tap_row(world, rotate);
    assert_eq!(
        world.get::<Children>(list).unwrap().0,
        [original[1], original[2], original[0]]
    );
    assert!(row_completed(world, original[0]));
    assert_eq!(row_label(world, original[0]), "Buy milk");
    assert_eq!(label_text(world, summary), "2 REMAINING");

    tap_row(world, clear);
    assert!(!world.is_alive(original[0]));
    assert_eq!(
        world.get::<Children>(list).unwrap().0,
        [original[1], original[2]]
    );
    tap_row(world, add);
    let added = world.get::<Children>(list).unwrap().0[2];
    assert_ne!(added, original[0]);
    assert_eq!(row_label(world, added), "Read a book");
    assert!(!row_completed(world, added));
    assert_eq!(label_text(world, summary), "3 REMAINING");
    tap_row(world, added);
    assert!(row_completed(world, added));
    assert!(!row_completed(world, original[1]));
    assert_eq!(label_text(world, summary), "2 REMAINING");
}

#[test]
fn collection_controls_handle_capacity_emptying_and_refilling() {
    let (mut app, summary, _) = todo_app();
    let world = &mut app.world;
    let (list, [add, rotate, clear]) = list_and_controls(world, summary);
    for _ in 0..4 {
        tap_row(world, add);
    }
    let full = world.get::<Children>(list).unwrap().0.clone();
    assert_eq!(full.len(), 6);
    assert_eq!(label_text(world, summary), "6 REMAINING");
    for row in &full {
        tap_row(world, *row);
    }
    assert_eq!(label_text(world, summary), "0 REMAINING");
    tap_row(world, clear);
    assert!(world.get::<Children>(list).unwrap().0.is_empty());
    assert!(full.iter().all(|row| !world.is_alive(*row)));
    tap_row(world, rotate);
    tap_row(world, clear);
    tap_row(world, add);
    let rows = &world.get::<Children>(list).unwrap().0;
    assert_eq!(rows.len(), 1);
    assert_eq!(row_label(world, rows[0]), "Buy milk");
    assert!(!row_completed(world, rows[0]));
    assert_eq!(label_text(world, summary), "1 REMAINING");
}

#[test]
fn collection_changes_relayout_rows_and_keep_overflow_scrollable() {
    use crate::input::event::scroll::scroll_bounds;
    use crate::ui::ComputedRect;

    let (mut app, summary, original) = todo_app();
    app.with_default_widgets();
    app.render_dirty().unwrap();
    let (list, [add, rotate, _]) = list_and_controls(&app.world, summary);
    let positions: [Rect; 3] =
        core::array::from_fn(|index| app.world.get::<ComputedRect>(original[index]).unwrap().0);
    assert!(positions.windows(2).all(|pair| pair[0].y < pair[1].y));
    tap_row(&mut app.world, rotate);
    app.render_dirty().unwrap();
    for (index, position) in positions.iter().enumerate() {
        assert_eq!(
            app.world
                .get::<ComputedRect>(original[(index + 1) % original.len()])
                .unwrap()
                .0,
            *position
        );
    }
    for _ in 0..3 {
        tap_row(&mut app.world, add);
    }
    app.render_dirty().unwrap();
    let list_rect = app.world.get::<ComputedRect>(list).unwrap().0;
    let last = *app.world.get::<Children>(list).unwrap().0.last().unwrap();
    let last_rect = app.world.get::<ComputedRect>(last).unwrap().0;
    let bounds = scroll_bounds(&app.world, list).unwrap();
    assert!(bounds.max_y > Fixed::ZERO);
    assert_eq!(
        bounds.content_height,
        last_rect.y + last_rect.h - list_rect.y
    );
    assert!(app.world.get::<Style>(list).unwrap().clip_children);
    assert!(list_rect.y + list_rect.h <= Fixed::from_int(280));
}
