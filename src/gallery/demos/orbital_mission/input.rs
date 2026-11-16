use crate::ecs::DeltaTimeMs;
use crate::gallery::play::orbit::{OrbitModal, OrbitModel, OrbitPage};
use crate::prelude::*;

#[mirui_macros::system(order = Animation)]
pub(super) fn orbit_tick_system(model: model!(OrbitModel), delta: res!(DeltaTimeMs)) {
    model.advance_ms(delta.0);
}

/// Desktop keys: the dial on the arrows, actions on letters, the dialog on
/// Enter and Esc.
#[event]
pub(super) fn keyboard(input: event!(Key), model: model!(OrbitModel)) -> bool {
    if !input.pressed
        || input
            .modifiers
            .is_some_and(|m| m.control || m.meta || m.alt)
    {
        return false;
    }
    if model.modal() != OrbitModal::None {
        match input.physical {
            Some(PhysicalKey::Enter | PhysicalKey::Space) => model.modal_primary(),
            Some(PhysicalKey::Escape) => model.set_modal(OrbitModal::None),
            _ => return false,
        };
        return true;
    }
    let step = if input.modifiers.is_some_and(|m| m.shift) {
        10
    } else {
        2
    };
    match input.physical {
        Some(PhysicalKey::Space | PhysicalKey::Enter) => model.press_run(),
        Some(PhysicalKey::KeyB) => model.press_burn(),
        Some(PhysicalKey::KeyE) => model.press_scan(),
        Some(PhysicalKey::KeyN) => model.press_schedule(),
        Some(PhysicalKey::ArrowUp | PhysicalKey::KeyW) => model.adjust_dv(step),
        Some(PhysicalKey::ArrowDown | PhysicalKey::KeyS) => model.adjust_dv(-step),
        Some(PhysicalKey::ArrowLeft | PhysicalKey::KeyA) => model.adjust_angle(-15),
        Some(PhysicalKey::ArrowRight | PhysicalKey::KeyD) => model.adjust_angle(15),
        Some(PhysicalKey::KeyF) => model.flip_angle(),
        Some(PhysicalKey::KeyR) => model.restart(),
        Some(PhysicalKey::KeyH) => model.set_modal(OrbitModal::Brief),
        Some(PhysicalKey::KeyM) => model.set_modal(OrbitModal::Missions),
        Some(PhysicalKey::Digit1) => model.set_page(OrbitPage::Map),
        Some(PhysicalKey::Digit2) => model.set_page(OrbitPage::Plan),
        Some(PhysicalKey::Digit3) => model.set_page(OrbitPage::Record),
        _ => return false,
    };
    true
}
