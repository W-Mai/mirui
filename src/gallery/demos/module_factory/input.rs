use super::state::FactoryNodes;
use crate::ecs::DeltaTimeMs;
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::factory::{
    FactoryModal, FactoryModel, FactoryPage, FactoryTool, GRID_WIDTH, ModuleKind,
};
use crate::input::event::gesture::GestureEvent;
use crate::prelude::*;
use crate::ui::ComputedRect;

fn local_point(world: &World, entity: Entity, x: Fixed, y: Fixed) -> Option<Point> {
    let rect = world.get::<ComputedRect>(entity)?.0;
    if rect.w.is_zero() || rect.h.is_zero() {
        return None;
    }
    Some(Point {
        x: (x - rect.x) * Fixed::from_int(480) / rect.w,
        y: (y - rect.y) * Fixed::from_int(320) / rect.h,
    })
}

fn tap_surface(model: &mut FactoryModel, point: Point) -> ChangeSet {
    if model.modal() != FactoryModal::None {
        return ChangeSet::NONE;
    }
    if model.page() == FactoryPage::Line {
        let x = point.x.to_int();
        let y = point.y.to_int();
        if (18..288).contains(&x) && (72..240).contains(&y) {
            let column = ((x - 18) / 30) as usize;
            let row = ((y - 72) / 28) as usize;
            return model
                .select_or_apply(row * GRID_WIDTH + column)
                .unwrap_or(ChangeSet::VISUAL);
        }
    }
    ChangeSet::NONE
}

pub(super) fn surface_gesture(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    let GestureEvent::Tap { x, y, .. } = event else {
        return false;
    };
    let Some(point) = local_point(world, entity, *x, *y) else {
        return false;
    };
    FactoryNodes::update(world, |model| tap_surface(model, point));
    true
}

pub(super) fn footer_action(world: &mut World, index: usize) {
    match index {
        0 => FactoryNodes::update(world, |model| model.open_modal(FactoryModal::Tools)),
        1 => {
            let tool = world
                .resource::<FactoryModel>()
                .map_or(FactoryTool::Select, FactoryModel::tool);
            if tool == FactoryTool::Select {
                FactoryNodes::result(world, FactoryModel::rotate_selected);
            } else {
                FactoryNodes::update(world, FactoryModel::rotate_tool);
            }
        }
        2 => FactoryNodes::update(world, FactoryModel::undo),
        3 => FactoryNodes::result(world, FactoryModel::step_once),
        _ => FactoryNodes::result(world, FactoryModel::toggle_run),
    }
}

pub(super) fn modal_action(world: &mut World, index: usize) {
    let modal = world
        .resource::<FactoryModel>()
        .map_or(FactoryModal::None, FactoryModel::modal);
    match modal {
        FactoryModal::Tools => {
            let tool = match index {
                0 => FactoryTool::Select,
                1 => FactoryTool::Build(ModuleKind::Belt),
                2 => FactoryTool::Build(ModuleKind::Furnace),
                3 => FactoryTool::Build(ModuleKind::Assembler),
                4 => FactoryTool::Build(ModuleKind::Inspector),
                _ => FactoryTool::Erase,
            };
            FactoryNodes::update(world, |model| model.set_tool(tool));
        }
        FactoryModal::Confirm { .. } if index == 0 => {
            FactoryNodes::update(world, FactoryModel::close_modal);
        }
        FactoryModal::Confirm { .. } if index == 1 => {
            FactoryNodes::update(world, FactoryModel::confirm_mission);
        }
        FactoryModal::Help => FactoryNodes::update(world, FactoryModel::close_modal),
        _ => {}
    }
}

#[mirui_macros::system(order = ANIMATION)]
pub(super) fn factory_tick_system(world: &mut World) {
    let elapsed = world.resource::<DeltaTimeMs>().map_or(16, |delta| delta.0);
    FactoryNodes::update(world, |model| model.advance_ms(elapsed));
}
