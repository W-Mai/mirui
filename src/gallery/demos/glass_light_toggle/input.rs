use super::geometry::Geometry;
use super::render::GlassToggle;
use crate::input::event::gesture::{DragCancel, DragEnd, DragMove, DragStart, Tap};
use crate::input::event::input::{Key, PhysicalKey};
use crate::input::event::press::{PressCancelled, Pressed, Released};
use crate::prelude::*;
use crate::ui::ComputedRect;

#[crate::event]
pub(super) fn tap(
    event: event!(Tap),
    entity: entity!(),
    values: query!(GlassToggle, ComputedRect),
) -> bool {
    let Some((glass, rect)) = values.get(entity) else {
        return false;
    };
    if !Geometry::new(rect.0).contains(event.x, event.y) {
        return false;
    }
    glass.model.toggle();
    true
}
#[crate::event]
pub(super) fn press(
    event: event!(Pressed),
    entity: entity!(),
    values: query!(GlassToggle, ComputedRect),
) {
    if let Some((glass, rect)) = values.get(entity) {
        let geometry = Geometry::new(rect.0);
        glass.model.pressed(
            geometry.contains(event.x, event.y),
            glass.model.knob_contains(geometry.local(event.x, event.y)),
        );
    }
}
#[crate::event]
pub(super) fn release(entity: entity!(), values: query!(GlassToggle), _event: event!(Released)) {
    if let Some(glass) = values.get(entity) {
        glass.model.release_press();
    }
}
#[crate::event]
pub(super) fn cancel_press(
    entity: entity!(),
    values: query!(GlassToggle),
    event: event!(PressCancelled),
) {
    let Some(glass) = values.get(entity) else {
        return;
    };
    if event.reason != crate::input::event::press::PressCancelReason::Gesture {
        glass.model.release_press();
    }
}
#[crate::event]
pub(super) fn drag_start(
    event: event!(DragStart),
    entity: entity!(),
    values: query!(GlassToggle, ComputedRect),
) -> bool {
    let Some((glass, rect)) = values.get(entity) else {
        return false;
    };
    let g = Geometry::new(rect.0);
    glass.model.begin_drag(g.local(event.x, event.y).x);
    glass.model.is_dragging()
}
#[crate::event]
pub(super) fn drag_move(
    event: event!(DragMove),
    entity: entity!(),
    values: query!(GlassToggle, ComputedRect),
) -> bool {
    let Some((glass, rect)) = values.get(entity) else {
        return false;
    };
    glass
        .model
        .drag(Geometry::new(rect.0).local(event.x, event.y).x);
    true
}
#[crate::event]
pub(super) fn drag_end(
    event: event!(DragEnd),
    entity: entity!(),
    values: query!(GlassToggle, ComputedRect),
) -> bool {
    let Some((glass, rect)) = values.get(entity) else {
        return false;
    };
    glass
        .model
        .end_drag(event.vx / Geometry::new(rect.0).scale, false);
    true
}
#[crate::event]
pub(super) fn drag_cancel(
    entity: entity!(),
    values: query!(GlassToggle),
    _event: event!(DragCancel),
) -> bool {
    let Some(glass) = values.get(entity) else {
        return false;
    };
    glass.model.end_drag(Fixed::ZERO, true);
    true
}
#[crate::event]
pub(super) fn key(entity: entity!(), values: query!(GlassToggle), event: event!(Key)) -> bool {
    let Some(glass) = values.get(entity) else {
        return false;
    };
    if !event.pressed || event.repeat == Some(true) {
        return false;
    }
    match event.physical {
        Some(PhysicalKey::Space | PhysicalKey::Enter) => glass.model.toggle(),
        Some(PhysicalKey::ArrowLeft) => glass.model.choose(false),
        Some(PhysicalKey::ArrowRight) => glass.model.choose(true),
        _ => return false,
    }
    true
}
