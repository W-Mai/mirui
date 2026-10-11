use super::state::FoldHintService;
use crate::gallery::play::expeditions::{Direction4, ExpeditionPanel, ExpeditionUiModel};
use crate::gallery::play::fold::FoldModel;
use crate::prelude::*;

#[event]
pub(super) fn keyboard(
    input: event!(TextCommitted),
    game: model!(FoldModel),
    expedition: model!(ExpeditionUiModel),
    hints: res!(FoldHintService),
) -> bool {
    if expedition.panel() != ExpeditionPanel::None {
        return false;
    }
    match input.text() {
        "w" | "W" => {
            game.move_direction(Direction4::Up);
        }
        "d" | "D" => {
            game.move_direction(Direction4::Right);
        }
        "s" | "S" => {
            game.move_direction(Direction4::Down);
        }
        "a" | "A" => {
            game.move_direction(Direction4::Left);
        }
        "z" | "Z" => {
            game.undo();
        }
        "h" | "H" => hints.request(game),
        _ => return false,
    }
    true
}
