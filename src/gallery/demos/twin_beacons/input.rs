use super::geometry::{CELL, SURFACE, cell_center, target_box};
use super::state::{StationTarget, SwipeStart, TwinHintService, TwinSurface, next};
use crate::gallery::play::expeditions::{
    Direction4, ExpeditionModal, ExpeditionPanel, ExpeditionUiModel, logical_point, twin_level,
};
use crate::gallery::play::twin::{TwinModel, pressed_for};
use crate::prelude::*;
use crate::ui::ComputedRect;

/// A drag shorter than this, in board pixels, is not a swipe.
const SWIPE_REACH: i32 = 18;

fn key_direction(key: PhysicalKey) -> Option<Direction4> {
    match key {
        PhysicalKey::KeyW | PhysicalKey::ArrowUp => Some(Direction4::Up),
        PhysicalKey::KeyD | PhysicalKey::ArrowRight => Some(Direction4::Right),
        PhysicalKey::KeyS | PhysicalKey::ArrowDown => Some(Direction4::Down),
        PhysicalKey::KeyA | PhysicalKey::ArrowLeft => Some(Direction4::Left),
        _ => None,
    }
}

#[event]
pub(super) fn keyboard(
    input: event!(Key),
    game: model!(TwinModel),
    expedition: model!(ExpeditionUiModel),
    hints: res!(TwinHintService),
) -> bool {
    if !input.pressed
        || input
            .modifiers
            .is_some_and(|m| m.control || m.meta || m.alt)
    {
        return false;
    }
    let Some(key) = input.physical else {
        return false;
    };
    if expedition.panel() != ExpeditionPanel::None {
        let closes = key == PhysicalKey::Escape
            || (key == PhysicalKey::Enter && expedition.panel() == ExpeditionPanel::Briefing);
        if closes {
            expedition.close();
        }
        return closes;
    }
    if let Some(direction) = key_direction(key) {
        game.move_direction(direction);
        return true;
    }
    match key {
        PhysicalKey::KeyZ => {
            game.undo();
        }
        PhysicalKey::KeyH => hints.request(game),
        PhysicalKey::KeyR => {
            game.restart();
        }
        PhysicalKey::Enter if game.modal() != ExpeditionModal::None => next(game, expedition),
        _ => return false,
    }
    true
}

/// A tap on a board moves its beacon one cell toward the tap; on board B
/// that presses whichever key sends B there.
#[event]
pub(super) fn board_tap(
    event: event!(Tap),
    node: entity!(),
    rows: query!(StationTarget, optional ComputedRect),
) -> bool {
    let Some((target, rect)) = rows.get(node) else {
        return false;
    };
    let station = usize::from(target.station);
    let (left, top, width, height) = target_box(station);
    let Some((x, y)) =
        rect.and_then(|rect| logical_point(rect.0, (event.x, event.y), (width, height)))
    else {
        return false;
    };
    if target.expedition.panel() != ExpeditionPanel::None
        || target.game.modal() != ExpeditionModal::None
    {
        return true;
    }
    let board = target.game.board();
    let beacon = cell_center(station, board.cells[station]);
    let (x, y) = (x + Fixed::from_int(left), y + Fixed::from_int(top));
    let reach = Fixed::from_ratio(CELL, 2);
    let Some(direction) = Direction4::toward(x - beacon.x, y - beacon.y, reach) else {
        return true;
    };
    let pressed = if station == 0 {
        direction
    } else {
        let mode = twin_level(board.shot.level).map_or(0, |level| level.mode());
        pressed_for(direction, mode)
    };
    target.game.move_direction(pressed);
    true
}

#[event]
pub(super) fn swipe_start(
    event: event!(DragStart),
    node: entity!(),
    rows: query!(mut SwipeStart),
) -> bool {
    let Some(start) = rows.get(node) else {
        return false;
    };
    start.0 = Some((event.x, event.y));
    true
}

/// A swipe anywhere on the surface presses the key it points to.
#[event]
pub(super) fn swipe_end(
    event: event!(DragEnd),
    node: entity!(),
    rows: query!(TwinSurface, mut SwipeStart, optional ComputedRect),
) -> bool {
    let Some((surface, start, rect)) = rows.get(node) else {
        return false;
    };
    let (Some(origin), Some(rect)) = (start.0.take(), rect) else {
        return false;
    };
    let from = logical_point(rect.0, origin, SURFACE);
    let to = logical_point(rect.0, (event.x, event.y), SURFACE);
    let (Some((x0, y0)), Some((x1, y1))) = (from, to) else {
        return false;
    };
    let reach = Fixed::from_int(SWIPE_REACH);
    let playing = surface.expedition.panel() == ExpeditionPanel::None
        && surface.game.modal() == ExpeditionModal::None;
    if playing {
        if let Some(direction) = Direction4::toward(x1 - x0, y1 - y0, reach) {
            surface.game.move_direction(direction);
        }
    }
    true
}

#[event]
pub(super) fn swipe_cancel(
    _event: event!(DragCancel),
    node: entity!(),
    rows: query!(mut SwipeStart),
) -> bool {
    if let Some(start) = rows.get(node) {
        start.0 = None;
    }
    true
}
