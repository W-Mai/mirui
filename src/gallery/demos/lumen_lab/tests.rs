use super::board::{LumenBoard, cell_center};
use super::runtime::lumen_tick_system;
use super::stars::LumenStars;
use super::{VIEWPORT, setup_app};
use crate::core::i18n::{I18n, Locale};
use crate::ecs::{DeltaTimeMs, SystemScheduler, World};
use crate::gallery::play::lumen::{LumenModel, LumenModelHandle, LumenNote};
use crate::input::event::gesture::GestureEvent;
use crate::input::event::{dispatch_input, input::PhysicalKey};
use crate::prelude::*;
use crate::ui::view::ViewRegistry;
use crate::ui::widgets::{Button, Text};
use crate::ui::{ComputedRect, Hidden};

fn fixture() -> World {
    let mut app = App::headless(VIEWPORT.0, VIEWPORT.1);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    ViewRegistry::reconcile_observations(&mut app.world);
    crate::core::reactive::flush_signal_dirty(&mut app.world);
    app.world
}

fn model(world: &World) -> LumenModelHandle {
    let board = world.find_by_id("lumen_board").unwrap();
    world.get::<LumenBoard>(board).unwrap().model.clone()
}

fn scan_phase(model: &LumenModelHandle) -> u16 {
    crate::core::model::ModelHandle::read(model, LumenModel::scan_phase)
}

/// Taps rotatable mirror `mirror` of the current puzzle.
fn turn(model: &LumenModelHandle, mirror: usize) {
    let position = crate::core::model::ModelHandle::read(model, |model| {
        model.level().mirrors[mirror].position
    });
    model.rotate_cell(position.x, position.y);
}

fn flush(world: &mut World) {
    crate::core::reactive::flush_signal_dirty(world);
}

fn set_locale(world: &mut World, locale: Locale) {
    world.resource::<I18n>().unwrap().set_locale(locale);
    flush(world);
}

fn tap(world: &mut World, event: &GestureEvent) -> bool {
    let decision = crate::__macro_support::dispatch_entity_gesture(world, event, 0).unwrap();
    crate::__macro_support::flush_entity_events(world).unwrap();
    decision == crate::input::event::BubbleControl::Prevent
}

fn tap_id(world: &mut World, id: &'static str) -> bool {
    let target = world.find_by_id(id).unwrap();
    tap(
        world,
        &GestureEvent::Tap {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target,
        },
    )
}

fn press(world: &mut World, key: PhysicalKey, at: u32) {
    dispatch_input(
        world,
        None,
        &crate::test_support::physical_key(key).into(),
        at,
        1,
        1,
    );
}

fn text_of(world: &World, id: &'static str) -> alloc::string::String {
    let entity = world.find_by_id(id).unwrap();
    let text = world.get::<Text>(entity).unwrap();
    assert!(text.has_valid_content(), "{id}");
    text.resolve(world.resource()).as_ref().into()
}

fn has_text(world: &World, wanted: &str) -> bool {
    world.query::<Text>().collect().into_iter().any(|entity| {
        world
            .get::<Text>(entity)
            .is_some_and(|text| text.resolve(world.resource()).as_ref() == wanted)
    })
}

fn is_hidden(world: &World, id: &'static str) -> bool {
    world.has::<Hidden>(world.find_by_id(id).unwrap())
}

#[test]
fn composition_uses_real_controls_and_one_dense_board() {
    let world = fixture();
    assert!(world.query::<Text>().collect().len() >= 20);
    assert!(world.query::<Button>().collect().len() >= 10);
    assert_eq!(world.query::<LumenBoard>().collect().len(), 1);
    assert_eq!(world.query::<LumenStars>().collect().len(), 2);
}

#[test]
fn grid_coordinates_promote_before_pixel_scaling() {
    assert_eq!(
        cell_center(crate::gallery::play::lumen::GridPoint { x: 6, y: 4 }),
        Point::new(345, 240)
    );
}

#[test]
fn tapping_a_mirror_uses_board_local_coordinates() {
    let mut world = fixture();
    let board = world.find_by_id("lumen_board").unwrap();
    world.insert(board, ComputedRect(Rect::new(40, 80, 756, 544)));
    assert!(tap(
        &mut world,
        &GestureEvent::Tap {
            x: Fixed::from_int(40 + (33 + 2 * 52) * 2),
            y: Fixed::from_int(80 + (32 + 3 * 52) * 2),
            target: board,
        }
    ));
    let model = model(&world);
    assert_eq!(model.moves(), 1);
    assert!(model.solved());
}

#[test]
fn the_margin_around_the_cells_is_not_a_cell() {
    let mut world = fixture();
    let board = world.find_by_id("lumen_board").unwrap();
    world.insert(board, ComputedRect(Rect::new(0, 0, 378, 272)));
    for (x, y) in [(3, 100), (100, 3), (375, 100), (100, 269)] {
        assert!(!tap(
            &mut world,
            &GestureEvent::Tap {
                x: Fixed::from_int(x),
                y: Fixed::from_int(y),
                target: board,
            }
        ));
    }
    assert_eq!(model(&world).moves(), 0);
}

#[test]
fn modal_blocks_board_taps_until_a_level_is_selected() {
    let mut world = fixture();
    let board = world.find_by_id("lumen_board").unwrap();
    world.insert(board, ComputedRect(Rect::new(0, 0, 378, 272)));
    let model = model(&world);
    model.open_levels();
    let before = model.moves();
    assert!(tap(
        &mut world,
        &GestureEvent::Tap {
            x: Fixed::from_int(2 * 52 + 33),
            y: Fixed::from_int(3 * 52 + 32),
            target: board,
        }
    ));
    assert_eq!(model.moves(), before);
}

#[test]
fn visual_changes_dirty_the_bound_board_without_node_sync() {
    use crate::ui::dirty::VisualDirty;

    let mut world = fixture();
    let board = world.find_by_id("lumen_board").unwrap();
    ViewRegistry::reconcile_observations(&mut world);
    world.remove::<VisualDirty>(board);

    model(&world).reveal_hint();
    flush(&mut world);

    assert!(world.has::<VisualDirty>(board));
}

#[test]
fn bound_tick_uses_the_frame_delta_and_zero_does_not_advance() {
    let mut world = fixture();
    let model = model(&world);
    let mut scheduler = SystemScheduler::new();
    scheduler.add(lumen_tick_system::system(model.clone()));

    world.insert_resource(DeltaTimeMs(400));
    scheduler.run_all(&mut world).unwrap();
    let drawn = scan_phase(&model);
    assert_eq!(drawn, 400 * 23, "the light flows once the beam is drawn");

    world.insert_resource(DeltaTimeMs(7));
    scheduler.run_all(&mut world).unwrap();
    assert_eq!(scan_phase(&model), drawn + 7 * 23);

    world.insert_resource(DeltaTimeMs(0));
    scheduler.run_all(&mut world).unwrap();
    assert_eq!(scan_phase(&model), drawn + 7 * 23);
}

#[test]
#[should_panic(expected = "requires resource")]
fn bound_tick_requires_the_frame_delta_resource() {
    let mut world = fixture();
    let model = model(&world);
    let mut scheduler = SystemScheduler::new();
    scheduler.add(lumen_tick_system::system(model));

    scheduler.run_all(&mut world).unwrap();
}

#[test]
fn status_names_where_the_beam_ends() {
    let mut world = fixture();
    let model = model(&world);
    assert_eq!(text_of(&world, "lumen_status"), "Off the board");
    assert_eq!(text_of(&world, "lumen_mirrors"), "Mirrors 1/2");
    assert_eq!(text_of(&world, "lumen_lamps"), "");
    assert_eq!(text_of(&world, "lumen_par"), "PAR 1");

    model.rotate_cell(2, 3);
    flush(&mut world);
    assert_eq!(text_of(&world, "lumen_status"), "Lit");
    assert_eq!(text_of(&world, "lumen_mirrors"), "Mirrors 2/2");
    assert_eq!(text_of(&world, "lumen_moves"), "01");

    model.select_level(10);
    flush(&mut world);
    assert_eq!(text_of(&world, "lumen_level_name"), "11 Shortcut");
    assert_eq!(text_of(&world, "lumen_status"), "A lamp is dark");
    assert_eq!(text_of(&world, "lumen_mirrors"), "Mirrors 1/4");
    assert_eq!(text_of(&world, "lumen_lamps"), "Lamps 0/1");
    assert_eq!(text_of(&world, "lumen_par"), "PAR 4");
}

#[test]
fn hint_and_fixed_mirror_notes_replace_the_tip() {
    let mut world = fixture();
    let model = model(&world);
    assert_eq!(
        text_of(&world, "lumen_note"),
        "Tap a mirror to flip it: / or \\."
    );
    model.reveal_hint();
    flush(&mut world);
    assert_eq!(text_of(&world, "lumen_note"), "Turn the marked mirror.");

    model.select_level(5);
    model.rotate_cell(3, 4);
    flush(&mut world);
    assert_eq!(model.note(), LumenNote::Fixed);
    assert_eq!(text_of(&world, "lumen_note"), "That mirror is fixed.");
}

#[test]
fn every_word_follows_the_page_language() {
    let mut world = fixture();
    set_locale(&mut world, Locale::ZhCn);
    assert_eq!(text_of(&world, "lumen_title"), "光路实验室");
    assert_eq!(text_of(&world, "lumen_level_name"), "01 转角");
    assert_eq!(text_of(&world, "lumen_status"), "光出界了");
    assert_eq!(text_of(&world, "lumen_mirrors"), "镜子 1/2");
    assert_eq!(text_of(&world, "lumen_par"), "最少 1");
    assert!(has_text(&world, "↺ 撤销") && has_text(&world, "☰ 选关"));

    let model = model(&world);
    model.rotate_cell(2, 3);
    model.finish_motion();
    flush(&mut world);
    assert_eq!(
        text_of(&world, "lumen_result_turns"),
        "转动 1 次 · 最少 1 次"
    );
    assert_eq!(text_of(&world, "lumen_next"), "下一关 ▶");

    set_locale(&mut world, Locale::EnUs);
    assert_eq!(text_of(&world, "lumen_title"), "LUMEN LAB");
    assert_eq!(text_of(&world, "lumen_next"), "Next ▶");
}

#[test]
fn level_picker_shows_every_puzzle_and_stars_the_solved_ones() {
    let mut world = fixture();
    let model = model(&world);
    model.open_levels();
    flush(&mut world);
    for level in 1..=15 {
        assert!(
            has_text(&world, &alloc::format!("{level:02}\n☆☆☆")),
            "{level}"
        );
    }
    assert!(has_text(&world, "Turning") && has_text(&world, "Fixed") && has_text(&world, "Lamps"));
    assert_eq!(text_of(&world, "lumen_levels_stars"), "★ 0 / 45");

    model.select_level(0);
    model.rotate_cell(2, 3);
    model.open_levels();
    flush(&mut world);
    assert!(has_text(&world, "01\n★★★"));
    assert!(!has_text(&world, "01\n☆☆☆"));
    assert_eq!(text_of(&world, "lumen_levels_stars"), "★ 3 / 45");
}

#[test]
fn a_solve_opens_the_result_card_and_next_moves_on() {
    let mut world = fixture();
    let model = model(&world);
    model.rotate_cell(2, 3);
    flush(&mut world);
    assert!(
        is_hidden(&world, "lumen_result"),
        "the beam is still drawing"
    );
    model.finish_motion();
    flush(&mut world);
    assert!(!is_hidden(&world, "lumen_result"));
    assert_eq!(text_of(&world, "lumen_result_turns"), "1 turn · par 1");
    assert_eq!(text_of(&world, "lumen_result_tip"), "All puzzles ★ 3 / 45");
    assert_eq!(text_of(&world, "lumen_next"), "Next ▶");

    assert!(tap_id(&mut world, "lumen_next"));
    flush(&mut world);
    assert_eq!(model.level_index(), 1);
    assert!(is_hidden(&world, "lumen_result"));

    model.select_level(14);
    model.reveal_hint();
    while let Some(mirror) = model.hint() {
        turn(&model, mirror);
        model.reveal_hint();
    }
    model.finish_motion();
    flush(&mut world);
    assert_eq!(model.earned(), 2);
    assert_eq!(text_of(&world, "lumen_result_tip"), "At par, no hint: ★★★");
    assert_eq!(text_of(&world, "lumen_next"), "☰ Puzzles");
}

#[test]
fn keys_play_the_board() {
    let mut world = fixture();
    let model = model(&world);
    press(&mut world, PhysicalKey::KeyH, 0);
    assert_eq!(model.hint(), Some(1));
    turn(&model, 1);
    press(&mut world, PhysicalKey::KeyZ, 1);
    assert_eq!(model.moves(), 0);
    turn(&model, 1);
    press(&mut world, PhysicalKey::Enter, 2);
    assert_eq!(model.level_index(), 1);
    turn(&model, 0);
    press(&mut world, PhysicalKey::KeyR, 3);
    assert_eq!(model.moves(), 0);
    press(&mut world, PhysicalKey::Escape, 4);
    assert!(model.levels_open());
    press(&mut world, PhysicalKey::KeyZ, 5);
    assert!(model.levels_open());
    press(&mut world, PhysicalKey::Escape, 6);
    assert!(!model.levels_open());
}
