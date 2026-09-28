use super::state::{OrbitNodes, mission_index};
use crate::ecs::DeltaTimeMs;
use crate::gallery::play::orbit::{OrbitModal, OrbitModel};
use crate::prelude::*;

pub(super) fn footer_action(world: &mut World, index: usize) {
    match index {
        0 => OrbitNodes::update(world, OrbitModel::toggle_running),
        1 => OrbitNodes::result(world, OrbitModel::burn),
        2 => OrbitNodes::result(world, OrbitModel::scan),
        3 => OrbitNodes::result(world, OrbitModel::schedule),
        _ => {
            let mission = mission_index(world);
            OrbitNodes::update(world, |model| model.set_modal(OrbitModal::Confirm(mission)));
        }
    }
}

pub(super) fn modal_action(world: &mut World, index: usize) {
    let modal = world
        .resource::<OrbitModel>()
        .map_or(OrbitModal::None, OrbitModel::modal);
    match modal {
        OrbitModal::Missions if index < 3 => OrbitNodes::update(world, |model| {
            model.set_modal(OrbitModal::Confirm(index as u8))
        }),
        OrbitModal::Missions if index == 3 => {
            OrbitNodes::update(world, |model| model.set_modal(OrbitModal::None));
        }
        OrbitModal::Confirm(_) if index == 0 => {
            OrbitNodes::update(world, |model| model.set_modal(OrbitModal::None))
        }
        OrbitModal::Confirm(mission) if index == 1 => {
            OrbitNodes::update(world, |model| model.load_mission(mission))
        }
        OrbitModal::Help => OrbitNodes::update(world, |model| model.set_modal(OrbitModal::None)),
        _ => {}
    }
}

#[mirui_macros::system(order = ANIMATION)]
pub(super) fn orbit_tick_system(world: &mut World) {
    let elapsed = world.resource::<DeltaTimeMs>().map_or(16, |delta| delta.0);
    OrbitNodes::update(world, |model| model.update(elapsed));
}

pub(super) fn cancel_node(world: &mut World, index: usize) {
    if let Some(node) = world
        .resource::<OrbitModel>()
        .and_then(|model| model.queue(index))
    {
        OrbitNodes::result(world, |model| model.cancel(node.id));
    }
}
