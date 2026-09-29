use super::state::FactorySurface;
use crate::core::model::ModelHandle;
use crate::ecs::DeltaTimeMs;
use crate::gallery::play::factory::{FactoryModal, FactoryModel, FactoryPage, GRID_WIDTH};
use crate::input::event::HandlerCtx;
use crate::input::event::gesture::GestureEvent;
use crate::prelude::{Fixed, Point, Rect};
use crate::ui::ComputedRect;

fn local_point(rect: Rect, x: Fixed, y: Fixed) -> Option<Point> {
    if rect.w.is_zero() || rect.h.is_zero() {
        return None;
    }
    Some(Point {
        x: (x - rect.x) * Fixed::from_int(480) / rect.w,
        y: (y - rect.y) * Fixed::from_int(320) / rect.h,
    })
}

pub(super) fn local_cell(model: &FactoryModel, point: Point) -> Option<usize> {
    if model.modal() != FactoryModal::None || model.page() != FactoryPage::Line {
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

pub(super) fn surface_gesture(ctx: &HandlerCtx<'_, GestureEvent>) -> bool {
    let GestureEvent::Tap { x, y, .. } = ctx.event else {
        return false;
    };
    let Some(model) = ctx
        .component::<FactorySurface>(ctx.entity)
        .map(|surface| surface.model.clone())
    else {
        return false;
    };
    let Some(rect) = ctx.component::<ComputedRect>(ctx.entity).map(|rect| rect.0) else {
        return false;
    };
    let Some(point) = local_point(rect, *x, *y) else {
        return false;
    };
    let Some(cell) = ModelHandle::read(&model, |model| local_cell(model, point)) else {
        return false;
    };
    let _ = model.select_or_apply(cell);
    true
}

#[mirui_macros::system(order = ANIMATION, bind(model))]
pub(super) fn factory_tick_system(model: &FactoryModel, delta: DeltaTimeMs) {
    model.advance_ms(delta.0);
}
