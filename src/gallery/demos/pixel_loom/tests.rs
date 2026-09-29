use super::input::{local_cell, pixel_tick_system};
use super::state::PixelSurface;
use super::{VIEWPORT, setup_app};
use crate::ecs::{DeltaTimeMs, SystemScheduler};
use crate::gallery::play::pixel::PixelModel;
use crate::input::event::GestureHandler;
use crate::input::event::gesture::GestureEvent;
use crate::prelude::*;
use crate::ui::ComputedRect;
use crate::ui::view::ViewRegistry;
use crate::ui::widgets::Text;

fn fixture() -> World {
    let mut app = App::headless(VIEWPORT.0, VIEWPORT.1);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    ViewRegistry::reconcile_observations(&mut app.world);
    crate::core::reactive::flush_signal_dirty(&mut app.world);
    app.world
}

fn with_model<R>(world: &World, inspect: impl FnOnce(&PixelModel) -> R) -> R {
    let surface = world.find_by_id("pixel_surface").unwrap();
    let model = &world.get::<PixelSurface>(surface).unwrap().model;
    crate::core::model::ModelHandle::read(model, inspect)
}

fn trigger(world: &mut World, surface: Entity, event: &GestureEvent) -> bool {
    GestureHandler::trigger(world, surface, event).unwrap_or(false)
}

#[test]
fn tick_system_uses_the_supplied_delta_and_preserves_zero() {
    let mut app = App::headless(VIEWPORT.0, VIEWPORT.1);
    let model = app.add_model(PixelModel::default());
    model.toggle_playback();
    let mut scheduler = SystemScheduler::new();
    scheduler.add(pixel_tick_system::system(model.clone()));

    app.world.insert_resource(DeltaTimeMs(0));
    scheduler.run_all(&mut app.world);
    assert_eq!(model.visible_frame(), 0);

    app.world.insert_resource(DeltaTimeMs(249));
    scheduler.run_all(&mut app.world);
    assert_eq!(model.visible_frame(), 0);

    app.world.insert_resource(DeltaTimeMs(1));
    scheduler.run_all(&mut app.world);
    assert_eq!(model.visible_frame(), 1);
}

#[test]
#[should_panic(expected = "missing system resource `DeltaTimeMs`")]
fn tick_system_rejects_a_missing_delta_resource() {
    let mut app = App::headless(VIEWPORT.0, VIEWPORT.1);
    let model = app.add_model(PixelModel::default());
    let mut scheduler = SystemScheduler::new();
    scheduler.add(pixel_tick_system::system(model));

    scheduler.run_all(&mut app.world);
}

#[test]
fn composition_uses_one_dense_surface_and_semantic_controls() {
    let world = fixture();
    assert!(world.find_by_id("pixel_surface").is_some());
    assert!(world.find_by_id("pixel_play").is_some());
    assert!(world.find_by_id("pixel_color_6").is_some());
    assert_eq!(world.query::<PixelSurface>().collect().len(), 1);
    for id in [
        "pixel_frame_status",
        "pixel_mode_status",
        "pixel_template_name",
        "pixel_fps",
        "pixel_play",
        "pixel_modal_title",
        "pixel_modal_subtitle",
    ] {
        let text = world.get::<Text>(world.find_by_id(id).unwrap()).unwrap();
        assert!(text.has_valid_content(), "{id}");
        assert_eq!(text.last_content_error(), None, "{id}");
    }
}

#[test]
fn tap_uses_the_bound_model_and_commits_one_undo_step() {
    let mut world = fixture();
    let surface = world.find_by_id("pixel_surface").unwrap();
    world.insert(surface, ComputedRect(Rect::new(0, 0, 480, 320)));
    let before = with_model(&world, |model| *model.frames());

    assert!(trigger(
        &mut world,
        surface,
        &GestureEvent::Tap {
            x: Fixed::from_int(18),
            y: Fixed::from_int(60),
            target: surface,
        },
    ));
    assert_eq!(with_model(&world, PixelModel::history_len), 1);
    assert_ne!(with_model(&world, |model| *model.frames()), before);

    world.get::<PixelSurface>(surface).unwrap().model.undo();
    assert_eq!(with_model(&world, |model| *model.frames()), before);
}

#[test]
fn cancelled_drag_restores_the_whole_stroke() {
    let mut world = fixture();
    let surface = world.find_by_id("pixel_surface").unwrap();
    let rect = Rect::new(0, 0, 480, 320);
    world.insert(surface, ComputedRect(rect));
    assert_eq!(
        local_cell(rect, Fixed::from_int(16), Fixed::from_int(59)),
        None
    );
    let before = with_model(&world, |model| *model.frames());
    assert!(trigger(
        &mut world,
        surface,
        &GestureEvent::DragStart {
            x: Fixed::from_int(18),
            y: Fixed::from_int(60),
            target: surface,
        },
    ));
    assert!(trigger(
        &mut world,
        surface,
        &GestureEvent::DragMove {
            x: Fixed::from_int(193),
            y: Fixed::from_int(235),
            dx: Fixed::from_int(175),
            dy: Fixed::from_int(175),
            target: surface,
        },
    ));
    assert!(trigger(
        &mut world,
        surface,
        &GestureEvent::DragCancel {
            x: Fixed::from_int(193),
            y: Fixed::from_int(235),
            target: surface,
        },
    ));
    assert_eq!(with_model(&world, |model| *model.frames()), before);
}

#[test]
fn visual_changes_dirty_the_bound_surface_without_node_sync() {
    use crate::ui::dirty::VisualDirty;

    let mut world = fixture();
    let surface = world.find_by_id("pixel_surface").unwrap();
    ViewRegistry::reconcile_observations(&mut world);
    world.remove::<VisualDirty>(surface);

    world
        .get::<PixelSurface>(surface)
        .unwrap()
        .model
        .toggle_onion();
    crate::core::reactive::flush_signal_dirty(&mut world);

    assert!(world.has::<VisualDirty>(surface));
}
