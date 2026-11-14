use super::input::local_cell;
use super::render::{cell_origin, map_box};
use super::setup_app;
use super::state::EchoSurface;
use crate::ecs::World;
use crate::gallery::play::echo::tests::path_to;
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
    assert_eq!(text.resolve(world.resource()).as_ref(), expected, "{id}");
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
    assert_eq!(local_cell(map, rect, screen_x, screen_y), Some(position));
}

#[test]
fn keyboard_wait_advances_exactly_one_beat() {
    let mut world = fixture();
    crate::input::event::dispatch_input(
        &mut world,
        None,
        &crate::test_support::physical_key(PhysicalKey::Space).into(),
        0,
        480,
        320,
    );
    assert_eq!(with_model(&world, EchoModel::tick), 1);
    #[cfg(feature = "persistence")]
    {
        let mut bytes = alloc::vec::Vec::new();
        crate::core::model::ModelHandle::read(&model_handle(&world), |model| {
            model.encode_replay(&mut bytes)
        })
        .unwrap();
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
    assert_text(&world, "echo_run", "1 STEP / 1 ECHO");
    assert_text(&world, "echo_tape_0", "ECHO 1 · 01 BEATS");
    assert!(!world.has::<Hidden>(world.find_by_id("echo_tape_0").unwrap()));
    assert!(world.has::<Hidden>(world.find_by_id("echo_tape_1").unwrap()));
    assert_text(&world, "echo_modal_title", "ECHO TAPES");
    assert_text(
        &world,
        "echo_modal_subtitle",
        "Tap a tape to show only that echo's route.",
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
        assert!(text.capacity().is_some(), "{id}");
        assert!(text.has_valid_content(), "{id}");
        assert_eq!(text.last_content_error(), None, "{id}");
    }
}

fn walk(model: &EchoModelHandle, target: u8) {
    let route = crate::core::model::ModelHandle::read(model, |model| path_to(model, target));
    for direction in route.into_iter().flatten() {
        model.step(direction);
    }
}

/// Holds every plate with an echo, picks up the fragments still lying
/// around and walks out.
fn solve_room(model: &EchoModelHandle) {
    let room = crate::core::model::ModelHandle::read(model, |model| *model.room());
    for plate in &room.plates[..usize::from(room.gate_count)] {
        walk(model, *plate);
        model.rewind();
    }
    for (gem, cell) in room.gems[..usize::from(room.gem_count)].iter().enumerate() {
        if !crate::core::model::ModelHandle::read(model, |model| model.collected(gem)) {
            walk(model, *cell);
        }
    }
    walk(model, room.end);
}

#[test]
fn empty_tapes_say_how_to_record_one() {
    let mut world = fixture();
    let model = model_handle(&world);
    model.set_modal(EchoModal::Tapes);
    flush(&mut world);
    assert_text(
        &world,
        "echo_modal_subtitle",
        "No echoes yet. RECORD ECHO saves your route as one.",
    );
    assert_text(
        &world,
        "echo_message",
        "STEP ON A PLATE, THEN RECORD AN ECHO TO KEEP IT DOWN",
    );
}

#[test]
fn the_archive_ends_on_totals_without_a_button_that_does_nothing() {
    let mut world = fixture();
    let model = model_handle(&world);
    for room in 1..=12 {
        solve_room(&model);
        flush(&mut world);
        assert_text(
            &world,
            "echo_modal_title",
            &alloc::format!("ROOM {room:02} SOLVED"),
        );
        assert_text(&world, "echo_message", "EXIT REACHED · ROOM SOLVED");
        assert_text(
            &world,
            "echo_modal_primary",
            if room == 12 { "FINISH" } else { "NEXT ROOM" },
        );
        let primary = world.find_by_id("echo_modal_primary").unwrap();
        assert!(!world.has::<Hidden>(primary), "room {room}");
        model.continue_archive();
    }
    flush(&mut world);
    assert_text(&world, "echo_modal_title", "ALL 12 ROOMS SOLVED");
    assert_text(
        &world,
        "echo_modal_subtitle",
        "Totals for the whole archive.",
    );
    assert_text(&world, "echo_message", "ALL 12 ROOMS SOLVED");
    let primary = world.find_by_id("echo_modal_primary").unwrap();
    assert!(world.has::<Hidden>(primary));
    let stats = world
        .get::<Text>(world.find_by_id("echo_result_stats").unwrap())
        .unwrap()
        .resolve(world.resource())
        .into_owned();
    assert!(
        stats.starts_with("TOTAL ") && stats.ends_with(" ECHOES"),
        "{stats}"
    );
}
