use super::input::local_cell;
use super::render::{cell_origin, map_box};
use super::setup_app;
use super::state::EchoSurface;
use crate::core::i18n::{I18n, Locale};
use crate::ecs::World;
use crate::gallery::play::change::ChangeSet;
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

fn text_of(world: &World, id: &'static str) -> alloc::string::String {
    let text = world
        .get::<Text>(world.find_by_id(id).unwrap())
        .expect("text widget");
    assert!(text.has_valid_content(), "{id}");
    assert_eq!(text.last_content_error(), None, "{id}");
    text.resolve(world.resource()).into_owned()
}

fn assert_text(world: &World, id: &'static str, expected: &str) {
    assert_eq!(text_of(world, id), expected, "{id}");
}

fn hidden(world: &World, id: &'static str) -> bool {
    world.has::<Hidden>(world.find_by_id(id).unwrap())
}

#[test]
fn composition_uses_one_dense_board_and_semantic_controls() {
    let world = fixture();
    for id in [
        "echo_surface",
        "echo_rewind",
        "echo_wait",
        "echo_undo",
        "echo_restart",
        "echo_menu",
    ] {
        assert!(world.find_by_id(id).is_some(), "{id}");
    }
    assert_eq!(world.query::<EchoSurface>().collect().len(), 1);
    assert!(world.query::<Button>().collect().len() >= 11);
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
fn the_board_gets_most_of_the_screen() {
    let world = fixture();
    let map = with_model(&world, map_box);
    // The first room is four floor columns by five rows.
    assert!(map.size >= 48, "cell {}", map.size);
    assert!(map.size * 5 >= 240);
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
fn tapping_yourself_waits_and_a_swipe_steps_once() {
    let mut world = fixture();
    let surface = world.find_by_id("echo_surface").unwrap();
    world.insert(surface, ComputedRect(Rect::new(0, 0, 480, 320)));
    let model = model_handle(&world);
    let position = model.position();
    model.tap_cell(position);
    assert_eq!(model.tick(), 1);
    assert_eq!(model.position(), position);

    let before = model.tick();
    model.begin_swipe();
    model.swipe(4, 1);
    assert_eq!(model.tick(), before, "a short drag is not a swipe");
    model.swipe(0, -30);
    model.swipe(0, -60);
    model.end_swipe();
    let moved = model.tick() != before || with_model(&world, |model| model.nudge().1 > Fixed::ZERO);
    assert!(moved, "the swipe stepped or bumped once");
    assert!(model.tick() <= before + 1);
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
    model.set_modal(EchoModal::Menu);
    flush(&mut world);
    assert!(!world.has::<VisualDirty>(surface));
    assert!(world.has::<VisualDirty>(modal));
    assert!(!world.has::<Hidden>(modal));
}

#[test]
fn motion_repaints_until_it_settles() {
    let world = fixture();
    let model = model_handle(&world);
    // The first room eases in, then rests.
    while model.advance_ms(16) != ChangeSet::NONE {}
    model.step(Direction::Wait);
    assert!(with_model(&world, |model| model.slide().2 < Fixed::ONE));
    assert_ne!(model.advance_ms(16), ChangeSet::NONE);
    model.advance_ms(400);
    assert_eq!(with_model(&world, |model| model.slide().2), Fixed::ONE);
    assert_eq!(model.advance_ms(16), ChangeSet::NONE);
}

#[test]
fn observed_text_and_tape_visibility_follow_the_model() {
    let mut world = fixture();
    let model = model_handle(&world);
    assert!(!hidden(&world, "echo_tapes_empty"));
    assert!(hidden(&world, "echo_tape_0"));

    model.step(Direction::Wait);
    model.rewind();
    flush(&mut world);

    assert_text(&world, "echo_ghosts", "ECHOES 1/3");
    assert_text(&world, "echo_tape_0", "● 1 BEAT");
    assert!(hidden(&world, "echo_tapes_empty"));
    assert!(!hidden(&world, "echo_tape_0"));
    assert!(hidden(&world, "echo_tape_1"));
    assert_text(
        &world,
        "echo_message",
        "Echo recorded · it walks your route as you move",
    );

    model.toggle_peek_ghost(0);
    flush(&mut world);
    assert_text(&world, "echo_tape_0", "◉ 1 BEAT");
    assert!(with_model(&world, |model| model.replay_beat().is_some()));

    model.set_modal(EchoModal::Menu);
    flush(&mut world);
    assert_text(&world, "echo_modal_title", "HOW TO PLAY");
    assert!(!hidden(&world, "echo_menu_body"));
    assert!(hidden(&world, "echo_modal_primary"));

    for id in [
        "echo_room",
        "echo_memory",
        "echo_ghosts",
        "echo_tick",
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

#[test]
fn chinese_mode_translates_every_label() {
    let mut world = fixture();
    let model = model_handle(&world);
    assert_text(&world, "echo_room", "ROOM 01 / 12");
    world.resource::<I18n>().unwrap().set_locale(Locale::ZhCn);
    flush(&mut world);
    assert_text(&world, "echo_room", "密室 01 / 12");
    assert_text(&world, "echo_memory", "片段 0/2");
    assert_text(&world, "echo_message", "站上踏板，再录下回响替你压住。");
    assert_text(&world, "echo_rewind", "● 录下回响");

    model.step(Direction::Wait);
    model.rewind();
    model.set_modal(EchoModal::Menu);
    flush(&mut world);
    assert_text(&world, "echo_tape_0", "● 1 步");
    assert_text(&world, "echo_ghosts", "回响 1/3");
    assert_text(&world, "echo_modal_title", "规则");
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
fn the_rules_open_and_close_over_the_room() {
    let mut world = fixture();
    let model = model_handle(&world);
    model.set_modal(EchoModal::Menu);
    flush(&mut world);
    assert_text(
        &world,
        "echo_message",
        "Stand on a plate, then record an echo to hold it.",
    );
    assert!(!hidden(&world, "echo_modal_secondary"));
    assert_eq!(model.step(Direction::Wait), ChangeSet::NONE);
    model.set_modal(EchoModal::None);
    flush(&mut world);
    assert!(hidden(&world, "echo_modal"));
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
        assert_text(&world, "echo_message", "Exit reached · room solved");
        assert_text(
            &world,
            "echo_modal_primary",
            if room == 12 {
                "FINISH"
            } else {
                "NEXT ROOM ▶"
            },
        );
        assert!(!hidden(&world, "echo_modal_primary"), "room {room}");
        model.continue_archive();
    }
    flush(&mut world);
    assert_text(&world, "echo_modal_title", "ALL 12 ROOMS SOLVED");
    assert_text(&world, "echo_modal_subtitle", "Totals for all 12 rooms.");
    assert_text(&world, "echo_message", "All 12 rooms solved");
    assert!(hidden(&world, "echo_modal_primary"));
    let stats = text_of(&world, "echo_result_stats");
    assert!(
        stats.starts_with("TOTAL ") && stats.ends_with(" ECHOES"),
        "{stats}"
    );
}
