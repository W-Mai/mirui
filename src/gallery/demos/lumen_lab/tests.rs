use super::board::{LumenBoard, board_tap, board_view, cell_center};
use super::runtime::LumenNodes;
use super::shell::build_widgets;
use crate::gallery::play::lumen::LumenModel;
use crate::input::event::gesture::GestureEvent;
use crate::prelude::*;
use crate::ui::ComputedRect;
use crate::ui::view::ViewRegistry;
use crate::ui::widgets::{Button, Text};
use crate::ui::{IdMap, UiScope};

fn fixture() -> World {
    let mut world = World::new();
    let mut registry = ViewRegistry::with_builtins();
    registry.insert(board_view());
    world.insert_resource(registry);
    world.insert_resource(IdMap::new());
    world.insert_resource(LumenModel::new());
    let root = WidgetBuilder::new(&mut world).id();
    let mut cx = UiScope::new(&mut world, root);
    build_widgets(&mut cx);
    let find = |id| world.find_by_id(id).unwrap();
    world.insert_resource(LumenNodes {
        board: find("lumen_board"),
        level_name: find("lumen_level_name"),
        level_subtitle: find("lumen_level_subtitle"),
        puzzle: find("lumen_puzzle"),
        status: find("lumen_status"),
        status_subtitle: find("lumen_status_subtitle"),
        moves: find("lumen_moves"),
        completed: find("lumen_completed"),
        hint_note: find("lumen_hint_note"),
        undo: find("lumen_undo"),
        scan: find("lumen_scan"),
        next: find("lumen_next"),
        modal: find("lumen_levels_modal"),
        mirror_labels: core::array::from_fn(|index| {
            world
                .find_by_id(match index {
                    0 => "lumen_mirror_1",
                    1 => "lumen_mirror_2",
                    2 => "lumen_mirror_3",
                    3 => "lumen_mirror_4",
                    4 => "lumen_mirror_5",
                    5 => "lumen_mirror_6",
                    _ => "lumen_mirror_7",
                })
                .unwrap()
        }),
        level_checks: core::array::from_fn(|index| {
            world
                .find_by_id(match index {
                    0 => "lumen_level_check_1",
                    1 => "lumen_level_check_2",
                    2 => "lumen_level_check_3",
                    3 => "lumen_level_check_4",
                    _ => "lumen_level_check_5",
                })
                .unwrap()
        }),
    });
    LumenNodes::sync(&mut world);
    world
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
    assert!(board_tap(
        &mut world,
        board,
        &GestureEvent::Tap {
            x: Fixed::from_int(40 + (27 + 2 * 38) * 2),
            y: Fixed::from_int(80 + (26 + 3 * 38) * 2),
            target: board,
        }
    ));
    let model = world.resource::<LumenModel>().unwrap();
    assert_eq!(model.moves(), 1);
    assert!(model.trace().solved);
}

#[test]
fn modal_blocks_board_taps_until_a_level_is_selected() {
    let mut world = fixture();
    let board = world.find_by_id("lumen_board").unwrap();
    world.insert(board, ComputedRect(Rect::new(0, 0, 282, 204)));
    LumenNodes::update(&mut world, LumenModel::open_levels);
    let before = world.resource::<LumenModel>().unwrap().moves();
    assert!(board_tap(
        &mut world,
        board,
        &GestureEvent::Tap {
            x: Fixed::from_int(2 * 38 + 27),
            y: Fixed::from_int(3 * 38 + 26),
            target: board,
        }
    ));
    assert_eq!(world.resource::<LumenModel>().unwrap().moves(), before);
}
