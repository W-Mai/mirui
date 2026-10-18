use super::state::TwinHintService;
use crate::gallery::play::expeditions::{Direction4, ExpeditionPanel, ExpeditionUiModel};
use crate::gallery::play::twin::TwinModel;
use crate::prelude::*;

#[event]
pub(super) fn keyboard(
    input: event!(Key),
    game: model!(TwinModel),
    expedition: model!(ExpeditionUiModel),
    hints: res!(TwinHintService),
) -> bool {
    if expedition.panel() != ExpeditionPanel::None {
        return false;
    }
    if !input.pressed
        || input
            .modifiers
            .is_some_and(|m| m.control || m.meta || m.alt)
    {
        return false;
    }
    match input.physical {
        Some(PhysicalKey::KeyW) => {
            game.move_direction(Direction4::Up);
        }
        Some(PhysicalKey::KeyD) => {
            game.move_direction(Direction4::Right);
        }
        Some(PhysicalKey::KeyS) => {
            game.move_direction(Direction4::Down);
        }
        Some(PhysicalKey::KeyA) => {
            game.move_direction(Direction4::Left);
        }
        Some(PhysicalKey::KeyZ) => {
            game.undo();
        }
        Some(PhysicalKey::KeyH) => hints.request(game),
        _ => return false,
    }
    true
}
