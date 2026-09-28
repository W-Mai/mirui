use super::board::{LumenBoard, cell_center};
use super::{VIEWPORT, setup_app};
use crate::gallery::play::lumen::LumenModelHandle;
use crate::input::event::GestureHandler;
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

fn tap_board(world: &mut World, board: Entity, event: &GestureEvent) -> bool {
    GestureHandler::trigger(world, board, event).unwrap_or(false)
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
        board,
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
        board,
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
