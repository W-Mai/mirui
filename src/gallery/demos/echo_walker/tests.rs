use super::input::{handle_key, local_cell};
use super::render::{cell_origin, map_box};
use super::setup_app;
use super::state::EchoSurface;
use crate::gallery::play::echo::{Direction, EchoModal, EchoModel, EchoModelHandle};
#[cfg(feature = "persistence")]
use crate::gallery::play::storage::{EchoReplayLog, ReplayKind};
use crate::prelude::*;
use crate::ui::dirty::VisualDirty;
use crate::ui::view::ViewRegistry;
use crate::ui::widgets::{Button, Text};
use crate::ui::{ComputedRect, Hidden};

fn fixture() -> World {
    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    ViewRegistry::reconcile_observations(&mut app.world);
    crate::core::reactive::flush_signal_dirty(&mut app.world);
    app.world
}

fn model_handle(world: &World) -> EchoModelHandle {
    let surface = world.find_by_id("echo_surface").unwrap();
    world.get::<EchoSurface>(surface).unwrap().model.clone()
}

fn with_model<R>(world: &World, inspect: impl FnOnce(&EchoModel) -> R) -> R {
    crate::core::model::ModelHandle::read(&model_handle(world), inspect)
}

fn flush(world: &mut World) {
    crate::core::reactive::flush_signal_dirty(world);
}

fn assert_text(world: &World, id: &'static str, expected: &str) {
    let text = world
        .get::<Text>(world.find_by_id(id).unwrap())
        .expect("text widget");
    assert_eq!(text.resolve(world).as_ref(), expected, "{id}");
    assert!(text.has_valid_content(), "{id}");
    assert_eq!(text.last_content_error(), None, "{id}");
}

#[test]
fn composition_uses_one_dense_board_and_semantic_controls() {
    let world = fixture();
    assert!(world.find_by_id("echo_surface").is_some());
    assert!(world.find_by_id("echo_rewind").is_some());
    assert_eq!(world.query::<EchoSurface>().collect().len(), 1);
    assert!(world.query::<Button>().collect().len() >= 15);
}

#[test]
fn board_hit_testing_maps_the_player_cell() {
    let mut world = fixture();
    let surface = world.find_by_id("echo_surface").unwrap();
    let rect = Rect::new(20, 30, 720, 480);
    world.insert(surface, ComputedRect(rect));
    let model = model_handle(&world);
    let (map, position) =
        crate::core::model::ModelHandle::read(&model, |model| (map_box(model), model.position()));
    let (x, y) = cell_origin(position, map);
    let screen_x = rect.x + Fixed::from_int(x + map.size / 2) * rect.w / Fixed::from_int(480);
    let screen_y = rect.y + Fixed::from_int(y + map.size / 2) * rect.h / Fixed::from_int(320);
    assert_eq!(
        crate::core::model::ModelHandle::read(&model, |model| {
            local_cell(model, rect, screen_x, screen_y)
        }),
        Some(position)
    );
}

#[test]
fn keyboard_wait_advances_exactly_one_beat() {
    let world = fixture();
    let model = model_handle(&world);
    assert!(handle_key(&model, ' '));
    assert_eq!(with_model(&world, EchoModel::tick), 1);
    #[cfg(feature = "persistence")]
    {
        let bytes = crate::core::model::ModelHandle::read(&model, EchoModel::encode_replay);
        let log = EchoReplayLog::decode(&bytes, ReplayKind::Echo).unwrap();
        assert_eq!(log.len(), 1);
    }
}

#[test]
fn visual_and_modal_sources_dirty_only_their_bound_views() {
    let mut world = fixture();
    let surface = world.find_by_id("echo_surface").unwrap();
    let modal = world.find_by_id("echo_modal").unwrap();
    let model = model_handle(&world);

    world.remove::<VisualDirty>(surface);
    world.remove::<VisualDirty>(modal);
    model.step(Direction::Wait);
    flush(&mut world);
    assert!(world.has::<VisualDirty>(surface));
    assert!(!world.has::<VisualDirty>(modal));

    world.remove::<VisualDirty>(surface);
    world.remove::<VisualDirty>(modal);
    model.set_modal(EchoModal::Tapes);
    flush(&mut world);
    assert!(!world.has::<VisualDirty>(surface));
    assert!(world.has::<VisualDirty>(modal));
    assert!(!world.has::<Hidden>(modal));
}

#[test]
fn observed_text_and_tape_visibility_follow_the_model() {
    let mut world = fixture();
    let model = model_handle(&world);

    model.step(Direction::Wait);
    model.rewind();
    model.set_modal(EchoModal::Tapes);
    flush(&mut world);

    assert_text(&world, "echo_ghosts", "ECHOES 1/3");
    assert_text(&world, "echo_run", "1 STEPS / 1 ECHOES");
    assert_text(&world, "echo_tape_0", "ECHO 1 · 01 BEATS");
    assert!(!world.has::<Hidden>(world.find_by_id("echo_tape_0").unwrap()));
    assert!(world.has::<Hidden>(world.find_by_id("echo_tape_1").unwrap()));
    assert_text(&world, "echo_modal_title", "ECHO TAPES");
    assert_text(
        &world,
        "echo_modal_subtitle",
        "Inspect each route. An echo holds its final cell.",
    );

    model.toggle_peek_ghost(0);
    flush(&mut world);
    assert_text(&world, "echo_tape_0", "ECHO 1 · 01 BEATS · SOLO");

    for id in [
        "echo_room",
        "echo_memory",
        "echo_ghosts",
        "echo_tick",
        "echo_run",
        "echo_message",
        "echo_modal_title",
        "echo_modal_subtitle",
        "echo_tape_0",
        "echo_tape_1",
        "echo_tape_2",
        "echo_result_stats",
        "echo_modal_primary",
    ] {
        let text = world.get::<Text>(world.find_by_id(id).unwrap()).unwrap();
        assert!(text.text_capacity().is_some(), "{id}");
        assert!(text.has_valid_content(), "{id}");
        assert_eq!(text.last_content_error(), None, "{id}");
    }
}
