use super::geometry::{SURFACE, hull_rect, panel_box};
use super::state::{FoldHintService, FoldSurface, MapTarget, next};
use crate::gallery::play::expeditions::{
    Direction4, ExpeditionModal, ExpeditionPanel, ExpeditionUiModel, SwipeStart, logical_point,
};
use crate::gallery::play::fold::FoldModel;
use crate::prelude::*;
use crate::ui::ComputedRect;

#[event]
pub(super) fn keyboard(
    input: event!(Key),
    game: model!(FoldModel),
    expedition: model!(ExpeditionUiModel),
    hints: res!(FoldHintService),
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

/// A tap on the map rolls the hull toward it; a tap on the hull itself
/// does nothing.
#[event]
pub(super) fn map_tap(
    event: event!(Tap),
    node: entity!(),
    rows: query!(MapTarget, optional ComputedRect),
) -> bool {
    let Some((target, rect)) = rows.get(node) else {
        return false;
    };
    let (left, top, width, height) = panel_box();
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
    let (x, y) = (x + Fixed::from_int(left), y + Fixed::from_int(top));
    let board = target.game.board();
    let hull = hull_rect(board.cells, board.len);
    if x >= hull.x && y >= hull.y && x < hull.x + hull.w && y < hull.y + hull.h {
        return true;
    }
    let half = Fixed::from_int(2);
    let (cx, cy) = (hull.x + hull.w / half, hull.y + hull.h / half);
    if let Some(direction) = Direction4::toward(x - cx, y - cy, Fixed::ONE) {
        target.game.move_direction(direction);
    }
    true
}

/// A swipe anywhere on the surface rolls the hull the way it points.
#[event]
pub(super) fn swipe_end(
    event: event!(DragEnd),
    node: entity!(),
    rows: query!(FoldSurface, mut SwipeStart, optional ComputedRect),
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
