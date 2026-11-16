use super::geometry::{CELL, SURFACE, cell_center, target_box};
use super::state::{StationTarget, TwinHintService, TwinSurface, next};
use crate::gallery::play::expeditions::{
    Direction4, ExpeditionModal, ExpeditionPanel, ExpeditionUiModel, SwipeStart, logical_point,
    twin_level,
};
use crate::gallery::play::twin::{TwinModel, pressed_for};
use crate::prelude::*;
use crate::ui::ComputedRect;

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
    if let Some(direction) = Direction4::from_key(key) {
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
    let Some(rect) = rect else {
        return false;
    };
    let swiped = start.finish(rect.0, (event.x, event.y), SURFACE);
    let playing = surface.expedition.panel() == ExpeditionPanel::None
        && surface.game.modal() == ExpeditionModal::None;
    if let (true, Some(direction)) = (playing, swiped) {
        surface.game.move_direction(direction);
    }
    true
}
