use crate::ecs::DeltaTimeMs;
use crate::gallery::play::lumen::LumenModel;
use crate::prelude::*;

#[mirui_macros::system(order = Animation)]
pub(super) fn lumen_tick_system(model: model!(LumenModel), delta: res!(DeltaTimeMs)) {
    model.advance_ms(delta.0);
}

/// Z undoes, H shows a hint, R restarts, Enter moves on from a solve, and
/// Esc or L opens and closes the puzzle list.
#[event]
pub(super) fn keyboard(input: event!(Key), model: model!(LumenModel)) -> bool {
    if !input.pressed
        || input
            .modifiers
            .is_some_and(|m| m.control || m.meta || m.alt)
    {
        return false;
    }
    if model.levels_open() {
        if matches!(
            input.physical,
            Some(PhysicalKey::Escape | PhysicalKey::KeyL)
        ) {
            model.close_levels();
            return true;
        }
        return false;
    }
    match input.physical {
        Some(PhysicalKey::KeyZ) => {
            model.undo();
        }
        Some(PhysicalKey::KeyH) => {
            model.reveal_hint();
        }
        Some(PhysicalKey::KeyR) => {
            model.reset();
        }
        Some(PhysicalKey::Enter | PhysicalKey::NumpadEnter) if model.solved() => {
            model.next_level();
        }
        Some(PhysicalKey::Escape | PhysicalKey::KeyL) => {
            model.open_levels();
        }
        _ => return false,
    }
    true
}
