use super::state::TideSurface;
use super::style::{BOARD_X, BOARD_Y, CELL, PITCH};
use crate::ecs::DeltaTimeMs;
use crate::gallery::play::tidal::{TideModal, TideModel, TideRecord};
use crate::prelude::*;

pub(super) fn local_cell(rect: Rect, x: Fixed, y: Fixed) -> Option<u8> {
    if rect.w.is_zero() || rect.h.is_zero() {
        return None;
    }
    let local_x = ((x - rect.x) * Fixed::from_int(480) / rect.w).to_int() - BOARD_X;
    let local_y = ((y - rect.y) * Fixed::from_int(320) / rect.h).to_int() - BOARD_Y;
    if local_x < 0 || local_y < 0 {
        return None;
    }
    let (column, row) = (local_x / PITCH, local_y / PITCH);
    if column >= 6 || row >= 6 || local_x % PITCH >= CELL || local_y % PITCH >= CELL {
        return None;
    }
    Some((row * 6 + column) as u8)
}

pub(super) fn surface_tap(surface: &TideSurface, rect: Rect, x: Fixed, y: Fixed) -> bool {
    let Some(cell) = local_cell(rect, x, y) else {
        return false;
    };
    surface.model.tap_cell(cell);
    true
}

/// Plays the drop and the harvest scores; idle frames leave the model alone.
#[mirui_macros::system(order = Animation)]
pub(super) fn tide_tick_system(model: model!(TideModel), delta: res!(DeltaTimeMs)) {
    if model.animating() {
        model.advance_ms(delta.0);
    }
}

/// Desktop keys: 1-3 pick a tile, the arrows walk the legal spots, Enter
/// places, Z undoes, R draws a new hand, V opens the voyage chart.
#[event]
pub(super) fn keyboard(
    input: event!(Key),
    model: model!(TideModel),
    record: model!(TideRecord),
) -> bool {
    if !input.pressed
        || input
            .modifiers
            .is_some_and(|m| m.control || m.meta || m.alt)
    {
        return false;
    }
    let digit = match input.physical {
        Some(PhysicalKey::Digit1) => Some(0),
        Some(PhysicalKey::Digit2) => Some(1),
        Some(PhysicalKey::Digit3) => Some(2),
        _ => None,
    };
    let confirm = matches!(
        input.physical,
        Some(PhysicalKey::Enter | PhysicalKey::Space)
    );
    match model.modal() {
        TideModal::None => {}
        TideModal::Voyage => {
            if !confirm
                && !matches!(
                    input.physical,
                    Some(PhysicalKey::Escape | PhysicalKey::KeyV)
                )
            {
                return false;
            }
            model.set_modal(TideModal::None);
            return true;
        }
        TideModal::Result => {
            if input.physical == Some(PhysicalKey::Escape) {
                model.set_modal(TideModal::None);
            } else if model.complete() && confirm {
                model.new_voyage();
                record.clear_fresh();
            } else if model.settled() && model.chapter() == 3 && confirm {
                model.continue_voyage(None);
                record.note(model.cumulative_score());
            } else if let Some(slot) = digit
                && model.settled()
                && model.chapter() < 3
            {
                model.continue_voyage(Some(model.perk_offers()[slot]));
            } else {
                return false;
            }
            return true;
        }
    }
    if let Some(slot) = digit {
        model.select_offer(slot as u8);
        return true;
    }
    match input.physical {
        Some(PhysicalKey::ArrowUp | PhysicalKey::KeyW) => model.step_pending(0, -1),
        Some(PhysicalKey::ArrowDown | PhysicalKey::KeyS) => model.step_pending(0, 1),
        Some(PhysicalKey::ArrowLeft | PhysicalKey::KeyA) => model.step_pending(-1, 0),
        Some(PhysicalKey::ArrowRight | PhysicalKey::KeyD) => model.step_pending(1, 0),
        Some(PhysicalKey::Enter | PhysicalKey::Space) if model.settled() || model.complete() => {
            model.set_modal(TideModal::Result)
        }
        Some(PhysicalKey::Enter | PhysicalKey::Space) => model.place_pending(),
        Some(PhysicalKey::Escape) => model.cancel_preview(),
        Some(PhysicalKey::KeyZ | PhysicalKey::KeyU) => model.undo(),
        Some(PhysicalKey::KeyR) => model.reroll(),
        Some(PhysicalKey::KeyV) => model.set_modal(TideModal::Voyage),
        _ => return false,
    };
    true
}
