use crate::ecs::DeltaTimeMs;
use crate::gallery::play::orbit::{OrbitModal, OrbitModel, OrbitModelHandle};

pub(super) fn footer_action(model: &OrbitModelHandle, index: usize) {
    match index {
        0 => {
            model.toggle_running();
        }
        1 => {
            let _ = model.burn();
        }
        2 => {
            let _ = model.scan();
        }
        3 => {
            let _ = model.schedule();
        }
        _ => {
            model.set_modal(OrbitModal::Confirm(model.mission()));
        }
    }
}

pub(super) fn modal_action(model: &OrbitModelHandle, index: usize) {
    match model.modal() {
        OrbitModal::Missions if index < 3 => {
            model.set_modal(OrbitModal::Confirm(index as u8));
        }
        OrbitModal::Missions if index == 3 => {
            model.set_modal(OrbitModal::None);
        }
        OrbitModal::Confirm(_) if index == 0 => {
            model.set_modal(OrbitModal::None);
        }
        OrbitModal::Confirm(mission) if index == 1 => {
            model.load_mission(mission);
        }
        OrbitModal::Help => {
            model.set_modal(OrbitModal::None);
        }
        _ => {}
    }
}

pub(super) fn cancel_node(model: &OrbitModelHandle, index: usize) {
    let _ = model.cancel_at(index);
}

#[mirui_macros::system(order = ANIMATION, bind(model))]
pub(super) fn orbit_tick_system(model: &OrbitModel, delta: Option<DeltaTimeMs>) {
    model.advance_ms(delta.map_or(16, |delta| delta.0));
}
