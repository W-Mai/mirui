use super::state::FactorySurface;
use crate::ecs::DeltaTimeMs;
use crate::gallery::play::factory::{FactoryModal, FactoryModel, FactoryPage, GRID_WIDTH};
use crate::prelude::{Fixed, Point, Rect};

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
    let x = point.x.to_int();
    let y = point.y.to_int();
    if !(18..288).contains(&x) || !(72..240).contains(&y) {
        return None;
    }
    let column = ((x - 18) / 30) as usize;
    let row = ((y - 72) / 28) as usize;
    Some(row * GRID_WIDTH + column)
}

pub(super) fn surface_tap(surface: &FactorySurface, rect: Rect, x: Fixed, y: Fixed) -> bool {
    let Some(point) = local_point(rect, x, y) else {
        return false;
    };
    let Some(cell) = local_cell(surface.model.page(), surface.model.modal(), point) else {
        return false;
    };
    let _ = surface.model.select_or_apply(cell);
    true
}

#[mirui_macros::system(order = Animation)]
pub(super) fn factory_tick_system(model: model!(FactoryModel), delta: res!(DeltaTimeMs)) {
    model.advance_ms(delta.0);
}
