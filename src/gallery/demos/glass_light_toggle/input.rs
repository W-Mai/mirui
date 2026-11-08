use super::state::{Geometry, GlassModel};
use crate::input::event::gesture::{DragCancel, DragEnd, DragMove, DragStart, Tap};
use crate::input::event::input::{Key, PhysicalKey};
use crate::input::event::press::{PressCancelled, Pressed, Released};
use crate::prelude::*;
use crate::ui::ComputedRect;

#[crate::event]
pub(super) fn tap(
    model: model!(GlassModel),
    event: event!(Tap),
    entity: entity!(),
    rects: query!(ComputedRect),
) -> bool {
    let Some(rect) = rects.get(entity) else {
        return false;
    };
    if !Geometry::new(rect.0).contains(event.x, event.y) {
        return false;
    }
    model.toggle();
    true
}
#[crate::event]
pub(super) fn press(
    model: model!(GlassModel),
    event: event!(Pressed),
    entity: entity!(),
    rects: query!(ComputedRect),
) {
    if let Some(rect) = rects.get(entity) {
        let geometry = Geometry::new(rect.0);
        model.pressed(
            geometry.contains(event.x, event.y),
            model.knob_contains(geometry.local(event.x, event.y)),
        );
    }
}
#[crate::event]
pub(super) fn release(model: model!(GlassModel), _event: event!(Released)) {
    model.release_press();
}
#[crate::event]
pub(super) fn cancel_press(model: model!(GlassModel), event: event!(PressCancelled)) {
    if event.reason != crate::input::event::press::PressCancelReason::Gesture {
        model.release_press();
    }
}
#[crate::event]
pub(super) fn drag_start(
    model: model!(GlassModel),
    event: event!(DragStart),
    entity: entity!(),
    rects: query!(ComputedRect),
) -> bool {
    let Some(rect) = rects.get(entity) else {
        return false;
    };
    let g = Geometry::new(rect.0);
    model.begin_drag(g.local(event.x, event.y).x);
    model.is_dragging()
}
#[crate::event]
pub(super) fn drag_move(
    model: model!(GlassModel),
    event: event!(DragMove),
    entity: entity!(),
    rects: query!(ComputedRect),
) -> bool {
    let Some(rect) = rects.get(entity) else {
        return false;
    };
    model.drag(Geometry::new(rect.0).local(event.x, event.y).x);
    true
}
#[crate::event]
pub(super) fn drag_end(
    model: model!(GlassModel),
    event: event!(DragEnd),
    entity: entity!(),
    rects: query!(ComputedRect),
) -> bool {
    let Some(rect) = rects.get(entity) else {
        return false;
    };
    model.end_drag(event.vx / Geometry::new(rect.0).scale, false);
    true
}
#[crate::event]
pub(super) fn drag_cancel(model: model!(GlassModel), _event: event!(DragCancel)) -> bool {
    model.end_drag(Fixed::ZERO, true);
    true
}
#[crate::event]
pub(super) fn key(model: model!(GlassModel), event: event!(Key)) -> bool {
    if !event.pressed || event.repeat == Some(true) {
        return false;
    }
    match event.physical {
        Some(PhysicalKey::Space | PhysicalKey::Enter) => model.toggle(),
        Some(PhysicalKey::ArrowLeft) => model.choose(false),
        Some(PhysicalKey::ArrowRight) => model.choose(true),
        _ => return false,
    }
    true
}
