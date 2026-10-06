use super::state::TwinHintService;
use crate::gallery::play::expeditions::{Direction4, ExpeditionPanel, ExpeditionUiModel};
use crate::gallery::play::twin::TwinModel;
use crate::prelude::*;

#[event]
pub(super) fn keyboard(
    input: event!(CharInput),
    game: model!(TwinModel),
    expedition: model!(ExpeditionUiModel),
    hints: res!(TwinHintService),
) -> bool {
    if expedition.panel() != ExpeditionPanel::None {
        return false;
    }
    match input.ch {
        'w' | 'W' => {
            game.move_direction(Direction4::Up);
        }
        'd' | 'D' => {
            game.move_direction(Direction4::Right);
        }
        's' | 'S' => {
            game.move_direction(Direction4::Down);
        }
        'a' | 'A' => {
            game.move_direction(Direction4::Left);
        }
        'z' | 'Z' => {
            game.undo();
        }
        'h' | 'H' => hints.request(game),
        _ => return false,
    }
    true
}
