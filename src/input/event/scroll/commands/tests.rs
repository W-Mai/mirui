use super::*;
use crate::ecs::commands::CommandQueue;
use crate::prelude::*;
use crate::surface::FramebufferAccess;
use crate::types::Transform;
use crate::ui::Hidden;

type TestApp = App<crate::surface::framebuf::FramebufSurface<fn(&[u8], PhysicalRect)>>;

fn setup() -> (TestApp, Entity, Entity, Entity) {
    let mut app = App::headless(120, 90);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    app.compose(root, |cx| {
        ui! {
            Column (id: "viewport", width: 80, height: 40, clip_children: true) [
                ScrollOffset::default(),
                ScrollConfig::default(),
            ] {
                View (width: 80, height: 60, shrink: 0.0)
                View (
                    id: "row",
                    width: 80,
                    height: 20,
                    shrink: 0.0,
                    bg_color: Color::rgb(0, 180, 120)
                )
                View (width: 80, height: 60, shrink: 0.0)
            }
        };
    });
    let container = app.world.find_by_id("viewport").unwrap();
    let row = app.world.find_by_id("row").unwrap();
    (app, root, container, row)
}

fn queue() -> CommandQueue {
    CommandQueue::bounded(CommandCapacity::new().scroll_into(2).scroll_to(2)).unwrap()
}

#[test]
fn reveal_and_align_use_new_layout_and_keep_dirty_pixels_equal_to_full() {
    let (mut app, _, container, row) = setup();
    let mut queue = queue();
    let mut batch = queue.begin();
    batch.commands().scroll_into(row);
    batch.finish(&mut app.world).unwrap();
    app.render().unwrap();
    assert_eq!(
        app.world.get::<ScrollOffset>(container).unwrap().y,
        40.into()
    );
    let mut batch = queue.begin();
    batch.commands().scroll_to(container, row);
    batch.finish(&mut app.world).unwrap();
    app.render_dirty().unwrap();
    assert_eq!(
        app.world.get::<ScrollOffset>(container).unwrap().y,
        60.into()
    );
    let pixels = app
        .backend
        .framebuffer()
        .read_pixels()
        .unwrap()
        .bytes()
        .to_vec();
    app.render().unwrap();
    assert_eq!(
        app.backend.framebuffer().read_pixels().unwrap().bytes(),
        pixels
    );
    let mut batch = queue.begin();
    batch.commands().scroll_to(
        container,
        ScrollOffset {
            x: 100.into(),
            y: Fixed::from_ratio(145, 2),
        },
    );
    batch.finish(&mut app.world).unwrap();
    app.render_dirty().unwrap();
    assert_eq!(
        *app.world.get::<ScrollOffset>(container).unwrap(),
        ScrollOffset {
            x: Fixed::ZERO,
            y: Fixed::from_ratio(145, 2)
        }
    );
}

#[test]
fn visible_targets_preserve_motion_and_warmed_requests_do_not_allocate() {
    let (mut app, _, container, row) = setup();
    let mut queue = queue();
    app.render().unwrap();
    let mut batch = queue.begin();
    batch.commands().scroll_into(row);
    batch.finish(&mut app.world).unwrap();
    app.render_dirty().unwrap();
    app.world
        .insert_resource(super::super::ScrollMotion::default());
    app.world
        .resource_mut::<super::super::ScrollMotion>()
        .unwrap()
        .target_entity = Some(container);
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            for _ in 0..8 {
                let mut batch = queue.begin();
                batch.commands().scroll_into(row);
                batch.finish(&mut app.world).unwrap();
                app.render_dirty().unwrap();
            }
        }),
        0
    );
    assert_eq!(
        app.world
            .resource::<super::super::ScrollMotion>()
            .unwrap()
            .target_entity,
        Some(container)
    );
}

#[test]
fn latest_request_wins_and_invalid_or_reparented_targets_are_not_reused() {
    let (mut app, root, container, row) = setup();
    let mut queue = queue();
    app.render().unwrap();
    let mut batch = queue.begin();
    batch.commands().scroll_to(container, row);
    batch.commands().scroll_to(
        container,
        ScrollOffset {
            x: Fixed::ZERO,
            y: 12.into(),
        },
    );
    batch.finish(&mut app.world).unwrap();
    app.render_dirty().unwrap();
    assert_eq!(
        app.world.get::<ScrollOffset>(container).unwrap().y,
        12.into()
    );
    let mut batch = queue.begin();
    batch.commands().scroll_into(row);
    batch.finish(&mut app.world).unwrap();
    app.world.insert(row, Parent(root));
    app.render_dirty().unwrap();
    assert_eq!(
        app.world.get::<ScrollOffset>(container).unwrap().y,
        12.into()
    );
    app.world.insert(row, Parent(container));
    let mut batch = queue.begin();
    batch.commands().scroll_into(row);
    batch.finish(&mut app.world).unwrap();
    app.world.insert(row, Hidden);
    app.render_dirty().unwrap();
    assert_eq!(
        app.world.get::<ScrollOffset>(container).unwrap().y,
        12.into()
    );
}

#[test]
fn user_takeover_cancels_pending_requests_before_the_next_preparation() {
    let (mut app, _, container, row) = setup();
    let mut queue = queue();
    app.render().unwrap();
    let mut batch = queue.begin();
    batch.commands().scroll_into(row);
    batch.finish(&mut app.world).unwrap();
    cancel(&mut app.world, container);
    app.render_dirty().unwrap();
    assert_eq!(
        app.world.get::<ScrollOffset>(container).unwrap().y,
        Fixed::ZERO
    );
    app.world.remove::<ScrollOffset>(container);
    crate::ui::lifecycle::prepare(&mut app.world);
    assert!(!app.world.has::<Pending>(container));
}

#[test]
fn nested_reveal_uses_resolved_inner_offsets_and_newly_changed_layout() {
    let mut app = App::headless(120, 90);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    app.compose(root, |cx| {
        ui! {
            Column (id: "outer", width: 80, height: 40, clip_children: true) [
                ScrollOffset::default(),
            ] {
                View (height: 60, shrink: 0.0)
                Column (id: "inner", width: 80, height: 40, shrink: 0.0, clip_children: true) [
                    ScrollOffset::default(),
                ] {
                    View (id: "spacer", height: 60, shrink: 0.0)
                    View (id: "target", height: 20, shrink: 0.0)
                    View (height: 60, shrink: 0.0)
                }
            }
        };
    });
    let inner = app.world.find_by_id("inner").unwrap();
    let outer = app.world.find_by_id("outer").unwrap();
    let target = app.world.find_by_id("target").unwrap();
    app.render().unwrap();
    let mut queue = queue();
    let mut batch = queue.begin();
    batch.commands().scroll_into(target);
    batch.finish(&mut app.world).unwrap();
    let spacer = app.world.find_by_id("spacer").unwrap();
    crate::ui::property::apply_to_world::<crate::ui::property::prop::Height>(
        &mut app.world,
        spacer,
        Dimension::px(90),
    );
    app.render_dirty().unwrap();
    assert_eq!(app.world.get::<ScrollOffset>(inner).unwrap().y, 70.into());
    assert_eq!(app.world.get::<ScrollOffset>(outer).unwrap().y, 60.into());
}

#[test]
fn transformed_targets_use_container_coordinates_and_oversize_aligns_once() {
    let (mut app, _, container, row) = setup();
    app.world.insert(
        container,
        crate::ui::widgets::WidgetTransform(Transform::scale(2.into(), 2.into())),
    );
    app.render().unwrap();
    let mut queue = queue();
    let mut batch = queue.begin();
    batch.commands().scroll_into(row);
    batch.finish(&mut app.world).unwrap();
    app.render_dirty().unwrap();
    assert_eq!(
        app.world.get::<ScrollOffset>(container).unwrap().y,
        40.into()
    );
    crate::ui::property::apply_to_world::<crate::ui::property::prop::Height>(
        &mut app.world,
        row,
        Dimension::px(80),
    );
    let mut batch = queue.begin();
    batch.commands().scroll_into(row);
    batch.finish(&mut app.world).unwrap();
    app.render_dirty().unwrap();
    assert_eq!(
        app.world.get::<ScrollOffset>(container).unwrap().y,
        60.into()
    );
    app.render_dirty().unwrap();
    assert_eq!(
        app.world.get::<ScrollOffset>(container).unwrap().y,
        60.into()
    );
}

use crate::ui::widgets::lazy_list::{CreateCell, LazyList, ListItems};

#[crate::event]
fn create_cell(event: event!(CreateCell)) -> Entity {
    let item = event.item.clone();
    ui! {
        View (
            bg_color: ${ if item.index() % 2 == 0 { Color::rgb(0, 120, 200) } else { Color::rgb(0, 180, 120) } }
        )
    }
}

#[crate::compose]
fn virtual_list(items: ListItems) -> Entity {
    ui! {
        LazyList (width: 80, height: 40, items: items, item_extent: 40) on CreateCell(create_cell::callback())
    }
}

#[test]
fn virtual_indices_realize_only_the_destination_and_reuse_the_cell_pool() {
    let mut app = App::headless(120, 90);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    let list = app.compose(root, virtual_list(65_536_u32.into()));
    let mut queue = queue();
    let mut batch = queue.begin();
    batch.commands().scroll_into((list, 100_u32));
    batch.finish(&mut app.world).unwrap();
    app.render().unwrap();
    assert_eq!(app.world.get::<ScrollOffset>(list).unwrap().y, 4000.into());
    let cells = app
        .world
        .get::<crate::ui::Children>(list)
        .unwrap()
        .0
        .clone();
    assert!(cells.len() <= 6);
    assert_eq!(app.world.get::<LazyList>(list).unwrap().visible_start, 100);
    let mut batch = queue.begin();
    batch.commands().scroll_to(list, 200_u32);
    batch.finish(&mut app.world).unwrap();
    app.render_dirty().unwrap();
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            for index in [200_u32, 100, 500, 250] {
                let mut batch = queue.begin();
                batch.commands().scroll_to(list, index);
                batch.finish(&mut app.world).unwrap();
                app.render_dirty().unwrap();
                assert_eq!(
                    app.world.get::<LazyList>(list).unwrap().visible_start,
                    index
                );
            }
        }),
        0
    );
    assert_eq!(app.world.get::<crate::ui::Children>(list).unwrap().0, cells);
}

#[crate::model]
#[derive(Debug)]
struct ItemData {
    #[observe]
    value: u8,
}

#[crate::model]
impl ItemData {}

#[test]
fn virtual_member_requests_follow_identity_and_reject_replaced_sources_or_providers() {
    use crate::core::model::ModelList;
    let mut app = App::headless(120, 90);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    let mut data = ModelList::with_capacity(8);
    for value in 0..8 {
        data.try_push(ItemData { value }).unwrap();
    }
    let data = app.add_model(data).finish();
    let list = app.compose(root, virtual_list(data.clone().into()));
    app.render().unwrap();
    let id = data.entries().nth(3).unwrap().0;
    let mut queue = queue();
    let mut batch = queue.begin();
    batch.commands().scroll_to(list, 3_u32);
    batch.finish(&mut app.world).unwrap();
    data.move_to(id, 5).unwrap();
    app.render_dirty().unwrap();
    assert_eq!(app.world.get::<ScrollOffset>(list).unwrap().y, 200.into());
    let mut batch = queue.begin();
    batch.commands().scroll_to(list, id);
    batch.finish(&mut app.world).unwrap();
    data.remove(id).unwrap();
    data.try_push(ItemData { value: 9 }).unwrap();
    app.render_dirty().unwrap();
    assert_eq!(app.world.get::<ScrollOffset>(list).unwrap().y, 200.into());
    let mut batch = queue.begin();
    batch.commands().scroll_to(list, 0_u32);
    batch.finish(&mut app.world).unwrap();
    app.world.get_mut::<LazyList>(list).unwrap().items = 20_u32.into();
    app.render_dirty().unwrap();
    assert_eq!(app.world.get::<ScrollOffset>(list).unwrap().y, 200.into());
    let mut batch = queue.begin();
    batch.commands().scroll_to(list, 0_u32);
    batch.finish(&mut app.world).unwrap();
    app.compose(root, |cx| {
        cx.on_event(list, LazyList::CreateCell, create_cell::callback());
    });
    app.render_dirty().unwrap();
    assert_eq!(app.world.get::<ScrollOffset>(list).unwrap().y, 200.into());
}
