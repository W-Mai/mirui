use super::*;
use crate::prelude::*;
use crate::types::Rect;
use crate::ui::widgets::Text;
use crate::ui::{Children, Parent};

#[crate::event]
fn row(event: event!(CreateCell)) -> Entity {
    let item = event.item.clone();
    ui! { Text(text: ${ if item.index() % 2 == 0 { "even" } else { "odd" } }) }
}

#[crate::compose]
fn panel(count: u32) -> Entity {
    ui! { LazyList(item_count: count, item_extent: Fixed::from_int(40)) on CreateCell(row::callback()) }
}

type TestApp =
    App<crate::surface::framebuf::FramebufSurface<fn(&[u8], crate::types::PhysicalRect)>>;
fn setup(count: u32) -> (TestApp, Entity) {
    let mut app = App::headless(200, 200);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    let list = app.compose(root, panel(count));
    (app, list)
}

fn viewport(world: &mut World, list: Entity, width: i32, height: i32, offset: i32) {
    world.insert(list, ComputedRect(Rect::new(0, 0, width, height)));
    world.get_mut::<ScrollOffset>(list).unwrap().y = offset.into();
    reconcile(world);
    crate::core::reactive::flush_signal_dirty(world);
}

#[test]
fn cells_wait_for_geometry_then_rebind_without_reconstruction() {
    let (mut app, list) = setup(100);
    assert!(!reconcile(&mut app.world));
    assert!(app.world.get::<Children>(list).unwrap().0.is_empty());
    viewport(&mut app.world, list, 200, 80, 20);
    let cells = app.world.get::<Children>(list).unwrap().0.clone();
    assert_eq!(cells.len(), 5);
    viewport(&mut app.world, list, 200, 80, 800);
    let expanded = app.world.get::<Children>(list).unwrap().0.clone();
    assert_eq!(expanded.len(), 6);
    viewport(&mut app.world, list, 200, 80, 1600);
    assert_eq!(app.world.get::<Children>(list).unwrap().0, expanded);
    for &cell in &expanded {
        let item = app.world.get::<CellItem>(cell).unwrap();
        let expected = if item.handle.index() % 2 == 0 {
            "even"
        } else {
            "odd"
        };
        assert_eq!(
            app.world.get::<Text>(cell).unwrap().resolve(&app.world),
            expected
        );
        assert_eq!(app.world.get::<Parent>(cell).unwrap().0, list);
    }
}

#[test]
fn resizing_keeps_intersecting_cell_identities() {
    let (mut app, list) = setup(100);
    viewport(&mut app.world, list, 200, 80, 820);
    let before = app.world.get::<CellPool>(list).unwrap().0.clone();
    viewport(&mut app.world, list, 200, 160, 820);
    let after = &app.world.get::<CellPool>(list).unwrap().0;
    for slot in before {
        assert!(
            after
                .iter()
                .any(|new| new.entity == slot.entity && new.index == slot.index)
        );
    }
}

#[test]
fn three_creation_forms_and_typed_builder_share_the_same_lifecycle() {
    #[crate::compose]
    fn forms() -> (Entity, Entity) {
        let block = ui! { LazyList(item_count: 10_u32, item_extent: Fixed::from_int(40)) on CreateCell {
            let item = event.item.clone();
            ui! { Text(text: ${ if item.index() == 0 { "first" } else { "next" } }) }
        }};
        let inline = ui! { LazyList(item_count: 10_u32, item_extent: Fixed::from_int(40)) on CreateCell(|event: event!(CreateCell)| {
            let item = event.item.clone();
            ui! { Text(text: ${ if item.index() == 0 { "first" } else { "next" } }) }
        }) };
        (block, inline)
    }
    let (mut app, named) = setup(10);
    let root = app.spawn_root().id();
    let (block, inline) = app.compose(root, forms());
    let builder = app.compose(root, |tree| {
        let list = tree.spawn(LazyList::new(10, 40));
        tree.on_event(list, LazyList::CreateCell, row::callback());
        list
    });
    for list in [named, block, inline, builder] {
        viewport(&mut app.world, list, 200, 80, 0);
        assert_eq!(app.world.get::<Children>(list).unwrap().0.len(), 4);
        assert!(!app.world.has::<crate::ui::HitTarget>(list));
    }
}

#[crate::event]
fn colored_row(event: event!(CreateCell)) -> Entity {
    let item = event.item.clone();
    ui! { View(bg_color: ${ if item.index() % 2 == 0 { ColorToken::Surface } else { ColorToken::SurfaceVariant } }) }
}

#[test]
fn warmed_rebinding_and_property_updates_allocate_nothing() {
    let mut app = App::headless(200, 200);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    let list = app.compose(root, |tree| {
        let list = tree.spawn(LazyList::new(1000, 40));
        tree.on_event(list, LazyList::CreateCell, colored_row::callback());
        list
    });
    viewport(&mut app.world, list, 200, 80, 820);
    viewport(&mut app.world, list, 200, 80, 1220);
    let count = app.world.get::<Children>(list).unwrap().0.len();
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            for offset in [820, 1220, 40, 0, -20, 3000, 39980, 40000, 100, 820] {
                viewport(&mut app.world, list, 200, 80, offset);
            }
        }),
        0
    );
    assert_eq!(app.world.get::<Children>(list).unwrap().0.len(), count);
}

#[test]
fn horizontal_partial_cells_and_elastic_bounds_use_the_same_axis_contract() {
    let (mut app, list) = setup(10);
    app.world.get_mut::<ScrollConfig>(list).unwrap().direction = ScrollAxis::Horizontal;
    app.world.get_mut::<ScrollOffset>(list).unwrap().x = 20.into();
    viewport(&mut app.world, list, 80, 200, 0);
    let pool = &app.world.get::<CellPool>(list).unwrap().0;
    assert!(pool.iter().any(|slot| slot.index == Some(0)));
    assert!(pool.iter().any(|slot| slot.index == Some(2)));
    for slot in pool {
        let layout = &app.world.get::<Style>(slot.entity).unwrap().layout;
        assert_eq!(layout.left, Dimension::px(slot.index.unwrap() as i32 * 40));
        assert_eq!(layout.width, Dimension::px(40));
    }
    let bounds = crate::input::event::scroll::scroll_bounds(&app.world, list).unwrap();
    assert_eq!(bounds.max_x, Fixed::from_int(320));
    assert_eq!(bounds.max_y, Fixed::ZERO);
    app.world.get_mut::<ScrollOffset>(list).unwrap().x = 420.into();
    reconcile(&mut app.world);
    assert!(
        app.world
            .get::<CellPool>(list)
            .unwrap()
            .0
            .iter()
            .all(|slot| slot.index.is_none())
    );
    assert_eq!(
        app.world.get::<ScrollOffset>(list).unwrap().x,
        Fixed::from_int(420)
    );
    app.world.get_mut::<LazyList>(list).unwrap().item_count = 0;
    assert_eq!(
        crate::input::event::scroll::scroll_bounds(&app.world, list)
            .unwrap()
            .max_x,
        Fixed::ZERO
    );
}

#[test]
fn reusing_a_pressed_cell_cancels_capture_without_stopping_the_list_scroll() {
    use crate::input::event::{
        focus::FocusState, gesture::GestureSystem, input::InputEvent, scroll::ScrollDragState,
    };
    let (mut app, list) = setup(100);
    viewport(&mut app.world, list, 200, 80, 0);
    let cell = app.world.get::<Children>(list).unwrap().0[0];
    let mut gestures = GestureSystem::default();
    gestures.recognizer.update(
        &InputEvent::PointerDown {
            id: 0,
            x: 10.into(),
            y: 10.into(),
        },
        1,
        Some(cell),
        &mut gestures.events,
    );
    app.world.insert_resource(gestures);
    app.world.insert_resource(FocusState {
        focused: Some(cell),
    });
    app.world.insert_resource(ScrollDragState {
        active: true,
        resolved: true,
        target: list,
        hit_entity: cell,
        ..Default::default()
    });
    viewport(&mut app.world, list, 200, 80, 800);
    assert!(
        app.world
            .resource::<FocusState>()
            .unwrap()
            .focused
            .is_none()
    );
    assert!(app.world.resource::<ScrollDragState>().unwrap().active);
    let gestures = app.world.resource_mut::<GestureSystem>().unwrap();
    gestures.recognizer.update(
        &InputEvent::PointerUp {
            id: 0,
            x: 10.into(),
            y: 10.into(),
        },
        2,
        None,
        &mut gestures.events,
    );
    assert!(gestures.events.buffer.is_empty());
}

#[test]
fn removing_the_list_releases_cell_model_registrations() {
    use crate::core::model::ModelHandle;
    let (mut app, list) = setup(100);
    viewport(&mut app.world, list, 200, 80, 0);
    let cell = app.world.get::<Children>(list).unwrap().0[0];
    let handle = app.world.get::<CellItem>(cell).unwrap().handle.clone();
    crate::ui::despawn_subtree(&mut app.world, list);
    assert!(!app.world.is_alive(cell));
    assert!(handle.access().upgrade().is_none());
}

#[crate::event]
fn interactive_row(_event: event!(CreateCell)) -> Entity {
    ui! { View() on Tap {} }
}

#[crate::compose]
fn clipped_list() -> Entity {
    ui! {
        View(position: Position::Absolute, left: 30, top: 30, width: 80, height: 80, clip_children: true) {
            LazyList(position: Position::Absolute, left: -20, top: -20, width: 120, height: 120,
                item_count: 100_u32, item_extent: Fixed::from_int(40)) on CreateCell(interactive_row::callback())
        }
    }
}

#[test]
fn first_layout_and_both_axes_share_clipping_with_pointer_hits() {
    use crate::input::event::hit_test::hit_test;
    use crate::types::Viewport;
    for axis in [ScrollAxis::Vertical, ScrollAxis::Horizontal] {
        let mut app = App::headless(200, 200);
        app.with_default_widgets();
        let root = app.spawn_root().id();
        let clip = app.compose(root, clipped_list());
        let list = app.world.get::<Children>(clip).unwrap().0[0];
        app.world.get_mut::<ScrollConfig>(list).unwrap().direction = axis;
        let offset = app.world.get_mut::<ScrollOffset>(list).unwrap();
        if axis == ScrollAxis::Vertical {
            offset.y = 20.into();
        } else {
            offset.x = 20.into();
        }
        crate::ui::render_system::try_update_layout(
            &mut app.world,
            root,
            &Viewport::new(200, 200, Fixed::ONE),
        )
        .unwrap();
        for (point, expected) in [(29, None), (30, Some(1)), (109, Some(2)), (110, None)] {
            let target = hit_test(&app.world, root, point.into(), point.into(), 200, 200);
            let index = target
                .and_then(|target| app.world.get::<CellItem>(target))
                .map(|item| item.handle.index());
            assert_eq!(index, expected, "axis {axis:?}, point {point}");
        }
    }
}

#[crate::event]
fn nested_row(_event: event!(CreateCell)) -> Entity {
    ui! { LazyList(item_count: 3_u32, item_extent: Fixed::from_int(20)) on CreateCell(interactive_row::callback()) }
}

#[test]
fn nested_lists_construct_and_settle_in_the_first_layout() {
    let mut app = App::headless(200, 200);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    let list = app.compose(root, |tree| {
        let list = tree.spawn(LazyList::new(2, 80));
        tree.on_event(list, LazyList::CreateCell, nested_row::callback());
        list
    });
    app.world.get_mut::<Style>(list).unwrap().layout.height = Dimension::px(160);
    crate::ui::render_system::try_update_layout(
        &mut app.world,
        root,
        &crate::types::Viewport::new(200, 200, Fixed::ONE),
    )
    .unwrap();
    let outer = &app.world.get::<Children>(list).unwrap().0;
    assert_eq!(outer.len(), 2);
    for &inner in outer {
        let cells = &app.world.get::<Children>(inner).unwrap().0;
        assert_eq!(cells.len(), 3);
        for &cell in cells {
            assert_eq!(
                app.world.get::<ComputedRect>(cell).unwrap().0.h,
                Fixed::from_int(20)
            );
        }
    }
}
