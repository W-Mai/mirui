use super::state::FactorySurface;
use super::style::{CELL, GRID_LEFT, GRID_TOP};
use crate::ecs::DeltaTimeMs;
use crate::gallery::play::factory::{
    FactoryModal, FactoryModel, FactoryPage, FactoryTool, GRID_HEIGHT, GRID_WIDTH,
};
use crate::prelude::*;

fn local_point(rect: Rect, x: Fixed, y: Fixed) -> Option<Point> {
    if rect.w.is_zero() || rect.h.is_zero() {
        return None;
    }
    Some(Point {
        x: (x - rect.x) * Fixed::from_int(480) / rect.w,
        y: (y - rect.y) * Fixed::from_int(320) / rect.h,
    })
}

pub(super) fn local_cell(page: FactoryPage, modal: FactoryModal, point: Point) -> Option<usize> {
    if modal != FactoryModal::None || page != FactoryPage::Line {
        return None;
    }
    let x = point.x.to_int() - GRID_LEFT;
    let y = point.y.to_int() - GRID_TOP;
    let width = GRID_WIDTH as i32 * CELL;
    let height = GRID_HEIGHT as i32 * CELL;
    if !(0..width).contains(&x) || !(0..height).contains(&y) {
        return None;
    }
    Some((y / CELL) as usize * GRID_WIDTH + (x / CELL) as usize)
}

pub(super) fn surface_tap(surface: &FactorySurface, rect: Rect, x: Fixed, y: Fixed) -> bool {
    let Some(point) = local_point(rect, x, y) else {
        return false;
    };
    let Some(cell) = local_cell(surface.model.page(), surface.model.modal(), point) else {
        return false;
    };
    surface.model.tap_cell(cell);
    true
}

/// Desktop keys: the arrows move the highlight, Enter uses the tool there
/// (or opens the next order after a win), R turns, Z undoes, Space runs or
/// pauses, T steps one tick and Esc puts the tool or a dialog away.
#[event]
pub(super) fn keyboard(input: event!(Key), model: model!(FactoryModel)) -> bool {
    if !input.pressed
        || input
            .modifiers
            .is_some_and(|m| m.control || m.meta || m.alt)
    {
        return false;
    }
    let modal = model.modal();
    match input.physical {
        Some(PhysicalKey::Escape) if modal != FactoryModal::None => {
            model.close_modal();
        }
        Some(PhysicalKey::Escape) => {
            model.set_tool(FactoryTool::Select);
        }
        Some(PhysicalKey::Enter | PhysicalKey::NumpadEnter) if modal == FactoryModal::Result => {
            model.select_modal_option(0);
        }
        _ if modal != FactoryModal::None => return false,
        Some(PhysicalKey::ArrowRight) => {
            model.move_selection(0);
        }
        Some(PhysicalKey::ArrowDown) => {
            model.move_selection(1);
        }
        Some(PhysicalKey::ArrowLeft) => {
            model.move_selection(2);
        }
        Some(PhysicalKey::ArrowUp) => {
            model.move_selection(3);
        }
        Some(PhysicalKey::Enter | PhysicalKey::NumpadEnter) if model.has_next() => {
            model.next_mission();
        }
        Some(PhysicalKey::Enter | PhysicalKey::NumpadEnter) => {
            model.tap_cell(model.selected());
        }
        Some(PhysicalKey::KeyR) => {
            let _ = model.rotate_active();
        }
        Some(PhysicalKey::KeyZ) => {
            model.undo();
        }
        Some(PhysicalKey::Space) => {
            let _ = model.toggle_run();
        }
        Some(PhysicalKey::KeyT) => {
            let _ = model.step_once();
        }
        _ => return false,
    }
    true
}

#[mirui_macros::system(order = Animation)]
pub(super) fn factory_tick_system(model: model!(FactoryModel), delta: res!(DeltaTimeMs)) {
    model.advance_ms(delta.0);
}
