use super::board::{LumenBoard, cell_center};
use super::runtime::lumen_tick_system;
use super::{VIEWPORT, setup_app};
use crate::ecs::{DeltaTimeMs, SystemScheduler, World};
use crate::gallery::play::lumen::{LumenModel, LumenModelHandle};
use crate::input::event::gesture::GestureEvent;
use crate::prelude::*;
use crate::ui::ComputedRect;
use crate::ui::view::ViewRegistry;
use crate::ui::widgets::{Button, Text};

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

fn tap_board(world: &mut World, event: &GestureEvent) -> bool {
    {
        let decision = crate::__macro_support::dispatch_entity_gesture(world, event, 0).unwrap();
        crate::__macro_support::flush_entity_events(world).unwrap();
        decision == crate::input::event::BubbleControl::Prevent
    }
}

#[test]
fn composition_uses_real_controls_and_one_dense_board() {
    let world = fixture();
    assert!(world.query::<Text>().collect().len() >= 20);
    assert!(world.query::<Button>().collect().len() >= 10);
    assert_eq!(world.query::<LumenBoard>().collect().len(), 1);
}

#[test]
fn grid_coordinates_promote_before_pixel_scaling() {
    assert_eq!(
        cell_center(crate::gallery::play::lumen::GridPoint { x: 6, y: 4 }),
        Point::new(255, 178)
    );
}

#[test]
fn tapping_a_mirror_uses_board_local_coordinates() {
    let mut world = fixture();
    let board = world.find_by_id("lumen_board").unwrap();
    world.insert(board, ComputedRect(Rect::new(40, 80, 564, 408)));
    assert!(tap_board(
        &mut world,
        &GestureEvent::Tap {
            x: Fixed::from_int(40 + (27 + 2 * 38) * 2),
            y: Fixed::from_int(80 + (26 + 3 * 38) * 2),
            target: board,
        }
    ));
    let model = model(&world);
    assert_eq!(model.moves(), 1);
    assert!(model.solved());
}

#[test]
fn modal_blocks_board_taps_until_a_level_is_selected() {
    let mut world = fixture();
    let board = world.find_by_id("lumen_board").unwrap();
    world.insert(board, ComputedRect(Rect::new(0, 0, 282, 204)));
    let model = model(&world);
    model.open_levels();
    let before = model.moves();
    assert!(tap_board(
        &mut world,
        &GestureEvent::Tap {
            x: Fixed::from_int(2 * 38 + 27),
            y: Fixed::from_int(3 * 38 + 26),
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

    model(&world).toggle_scan();
    crate::core::reactive::flush_signal_dirty(&mut world);

    assert!(world.has::<VisualDirty>(board));
}

#[test]
fn bound_tick_uses_the_frame_delta_and_zero_does_not_advance() {
    let mut world = fixture();
    let model = model(&world);
    let mut scheduler = SystemScheduler::new();
    scheduler.add(lumen_tick_system::system(model.clone()));

    world.insert_resource(DeltaTimeMs(7));
    scheduler.run_all(&mut world).unwrap();
    assert_eq!(scan_phase(&model), 7 * 23);

    world.insert_resource(DeltaTimeMs(0));
    scheduler.run_all(&mut world).unwrap();
    assert_eq!(scan_phase(&model), 7 * 23);
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

#[test]
fn status_names_where_the_beam_ends() {
    let mut world = fixture();
    let model = model(&world);
    assert_eq!(text_of(&world, "lumen_status"), "光出界了");
    assert_eq!(text_of(&world, "lumen_status_subtitle"), "MIRRORS 1 / 2");

    model.rotate_cell(2, 3);
    crate::core::reactive::flush_signal_dirty(&mut world);
    assert_eq!(text_of(&world, "lumen_status"), "接通");
    assert_eq!(text_of(&world, "lumen_status_subtitle"), "MIRRORS 2 / 2");

    model.select_level(10);
    crate::core::reactive::flush_signal_dirty(&mut world);
    assert_eq!(text_of(&world, "lumen_level_name"), "近路");
    assert_eq!(text_of(&world, "lumen_status"), "还有灯没亮");
    assert_eq!(
        text_of(&world, "lumen_status_subtitle"),
        "MIRRORS 1/4  LAMPS 0/1"
    );
    assert_eq!(text_of(&world, "lumen_puzzle"), "PUZZLE 11 / 15");
}

#[test]
fn hint_and_fixed_mirror_notes_replace_the_tip() {
    let mut world = fixture();
    let model = model(&world);
    assert_eq!(text_of(&world, "lumen_hint_note"), "每面镜子都会用上");
    model.reveal_hint();
    crate::core::reactive::flush_signal_dirty(&mut world);
    assert_eq!(text_of(&world, "lumen_hint_note"), "转动高亮的镜子");

    model.select_level(5);
    model.rotate_cell(3, 4);
    crate::core::reactive::flush_signal_dirty(&mut world);
    assert_eq!(text_of(&world, "lumen_hint_note"), "这面镜子是固定的");
}

#[test]
fn level_picker_shows_every_puzzle_and_stars_the_solved_ones() {
    let mut world = fixture();
    let model = model(&world);
    model.open_levels();
    crate::core::reactive::flush_signal_dirty(&mut world);
    for level in 1..=15 {
        assert!(
            has_text(&world, &alloc::format!("{level:02}\n☆")),
            "{level}"
        );
    }
    assert!(has_text(&world, "转镜") && has_text(&world, "定镜") && has_text(&world, "点灯"));
    assert_eq!(text_of(&world, "lumen_levels_stars"), "★ 0 / 15");

    model.select_level(0);
    model.rotate_cell(2, 3);
    model.open_levels();
    crate::core::reactive::flush_signal_dirty(&mut world);
    assert!(has_text(&world, "01\n★"));
    assert!(!has_text(&world, "01\n☆"));
    assert_eq!(text_of(&world, "lumen_levels_stars"), "★ 1 / 15");
}
