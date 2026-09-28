use super::state::PostSurface;
use super::{VIEWPORT, setup_app};
use crate::gallery::play::post::{PostModel, PostModelHandle};
use crate::input::event::GestureHandler;
use crate::input::event::gesture::GestureEvent;
use crate::prelude::*;
use crate::ui::view::ViewRegistry;
use crate::ui::widgets::Text;
use crate::ui::{ComputedRect, Hidden};

fn fixture(width: u16, height: u16) -> World {
    let mut app = App::headless(width, height);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    ViewRegistry::reconcile_observations(&mut app.world);
    crate::core::reactive::flush_signal_dirty(&mut app.world);
    app.world
}

fn with_model<R>(world: &World, inspect: impl FnOnce(&PostModel) -> R) -> R {
    let surface = world.find_by_id("post_surface").unwrap();
    let model = &world.get::<PostSurface>(surface).unwrap().model;
    crate::core::model::ModelHandle::read(model, inspect)
}

fn model_handle(world: &World) -> PostModelHandle {
    let surface = world.find_by_id("post_surface").unwrap();
    world.get::<PostSurface>(surface).unwrap().model.clone()
}

fn flush(world: &mut World) {
    crate::core::reactive::flush_signal_dirty(world);
}

fn assert_text(world: &World, id: &'static str, expected: &str) {
    let entity = world.find_by_id(id).unwrap();
    let text = world.get::<Text>(entity).unwrap();
    assert_eq!(text.resolve(world).as_ref(), expected, "{id}");
    assert!(text.has_valid_content(), "{id}");
    assert_eq!(text.last_content_error(), None, "{id}");
}

fn assert_bounded_text_valid(world: &World) {
    for id in [
        "post_sorted",
        "post_status",
        "post_misses",
        "post_station_0",
        "post_station_1",
        "post_station_2",
        "post_queue_0",
        "post_queue_1",
        "post_queue_2",
        "post_queue_3",
        "post_queue_4",
        "post_in_transit",
        "post_run",
        "post_speed",
        "post_modal_title",
        "post_modal_subtitle",
        "post_summary_score",
        "post_summary_correct",
        "post_summary_missed",
    ] {
        let entity = world.find_by_id(id).unwrap();
        let text = world.get::<Text>(entity).unwrap();
        assert!(text.text_capacity().is_some(), "{id}");
        assert!(text.has_valid_content(), "{id}");
        assert_eq!(text.last_content_error(), None, "{id}");
    }
}

fn is_hidden(world: &World, id: &'static str) -> bool {
    world.has::<Hidden>(world.find_by_id(id).unwrap())
}

fn trigger(world: &mut World, surface: Entity, event: &GestureEvent) -> bool {
    GestureHandler::trigger(world, surface, event).unwrap_or(false)
}

#[test]
fn composition_uses_one_dense_route_surface_and_semantic_controls() {
    let world = fixture(VIEWPORT.0, VIEWPORT.1);
    assert!(world.find_by_id("post_surface").is_some());
    assert!(world.find_by_id("post_run").is_some());
    assert!(world.find_by_id("post_manifest_2").is_some());
    assert_eq!(world.query::<PostSurface>().collect().len(), 1);
}

#[test]
fn switch_hit_targets_map_through_the_surface_geometry() {
    let mut world = fixture(960, 640);
    let surface = world.find_by_id("post_surface").unwrap();
    world.insert(surface, ComputedRect(Rect::new(20, 30, 720, 480)));
    let rect = world.get::<ComputedRect>(surface).unwrap().0;
    let x = rect.x + rect.w * Fixed::from_int(151) / Fixed::from_int(480);
    let y = rect.y + rect.h * Fixed::from_int(157) / Fixed::from_int(320);
    assert_eq!(with_model(&world, |model| model.switch(0)), 0);
    assert!(trigger(
        &mut world,
        surface,
        &GestureEvent::Tap {
            x,
            y,
            target: surface,
        },
    ));
    assert_eq!(with_model(&world, |model| model.switch(0)), 1);
}

#[test]
fn visual_changes_dirty_the_bound_surface_without_node_sync() {
    use crate::ui::dirty::VisualDirty;

    let mut world = fixture(VIEWPORT.0, VIEWPORT.1);
    let surface = world.find_by_id("post_surface").unwrap();
    ViewRegistry::reconcile_observations(&mut world);
    world.remove::<VisualDirty>(surface);

    world
        .get::<PostSurface>(surface)
        .unwrap()
        .model
        .toggle_switch(0);
    crate::core::reactive::flush_signal_dirty(&mut world);

    assert!(world.has::<VisualDirty>(surface));
}

#[test]
fn observed_state_updates_bounded_text_and_visibility() {
    let mut world = fixture(VIEWPORT.0, VIEWPORT.1);
    let model = model_handle(&world);

    assert_text(&world, "post_status", "准备好，拨动你的第一班轨道");
    assert!(is_hidden(&world, "post_modal"));
    assert_bounded_text_valid(&world);

    model.open_manifests();
    flush(&mut world);
    assert!(!is_hidden(&world, "post_modal"));
    assert!(!is_hidden(&world, "post_manifest_0"));
    assert!(is_hidden(&world, "post_summary_score"));
    assert_text(
        &world,
        "post_modal_subtitle",
        "更换班次会从头开始；没有时间惩罚，可以随时暂停。",
    );

    model.load_manifest(1);
    flush(&mut world);
    assert!(is_hidden(&world, "post_modal"));
    assert_text(&world, "post_sorted", "SORTED 0 / 12");

    model.load_manifest(2);
    model.spawn_manual();
    model.spawn_manual();
    flush(&mut world);
    assert_text(&world, "post_status", "已暂停调度");
    assert_text(&world, "post_in_transit", "2 / 3 在途");
    assert!(!is_hidden(&world, "post_queue_0"));
    assert!(is_hidden(&world, "post_queue_4"));

    model.open_reset();
    flush(&mut world);
    assert!(is_hidden(&world, "post_manifest_0"));
    assert!(!is_hidden(&world, "post_reset_confirm"));
    assert_bounded_text_valid(&world);
}
