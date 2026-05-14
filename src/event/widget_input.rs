use crate::components::button::Button;
use crate::components::checkbox::Checkbox;
use crate::components::progress_bar::ProgressBar;
use crate::ecs::{Entity, World};
use crate::types::Fixed;
use crate::widget::dirty::{Dirty, PrevRect};

use super::GestureHandler;
use super::gesture::GestureEvent;

/// Press feedback on Button: highlight while the gesture is in flight,
/// release on Tap / DragEnd. Without DragStart we'd never see a "held"
/// state because Tap is press+release in one event.
fn button_handler(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    match event {
        GestureEvent::DragStart { .. } => {
            if let Some(btn) = world.get_mut::<Button>(entity) {
                btn.pressed = true;
            }
            world.insert(entity, Dirty);
            false
        }
        GestureEvent::Tap { .. } | GestureEvent::DragEnd { .. } => {
            if let Some(btn) = world.get_mut::<Button>(entity) {
                btn.pressed = false;
            }
            world.insert(entity, Dirty);
            true
        }
        _ => false,
    }
}

fn checkbox_handler(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    if let GestureEvent::Tap { .. } = event {
        if let Some(cb) = world.get_mut::<Checkbox>(entity) {
            cb.toggle();
        }
        world.insert(entity, Dirty);
        return true;
    }
    false
}

/// ProgressBar reads the last rendered rect (PrevRect) so it can map
/// the pointer x to a 0..1 ratio without a layout re-walk.
fn progress_bar_handler(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    let x = match event {
        GestureEvent::Tap { x, .. } | GestureEvent::DragMove { x, .. } => *x,
        _ => return false,
    };
    let Some(rect) = world.get::<PrevRect>(entity).map(|p| p.0) else {
        return false;
    };
    if rect.w <= Fixed::ZERO {
        return false;
    }
    let ratio = ((x - rect.x).to_f32() / rect.w.to_f32()).clamp(0.0, 1.0);
    if let Some(pb) = world.get_mut::<ProgressBar>(entity) {
        pb.value = ratio;
    }
    world.insert(entity, Dirty);
    true
}

/// Attach the appropriate GestureHandler to any Button/Checkbox/
/// ProgressBar entity that doesn't already have one. Call once after
/// building the widget tree (idempotent — handlers are skipped if
/// present so user-supplied overrides win).
pub fn attach_widget_input_handlers(world: &mut World, root: Entity) {
    let mut stack = alloc::vec::Vec::with_capacity(16);
    stack.push(root);
    while let Some(entity) = stack.pop() {
        if let Some(children) = world.get::<crate::widget::Children>(entity) {
            for &child in &children.0 {
                stack.push(child);
            }
        }

        if world.get::<GestureHandler>(entity).is_some() {
            continue;
        }
        if world.get::<Button>(entity).is_some() {
            world.insert(
                entity,
                GestureHandler {
                    on_gesture: button_handler,
                },
            );
        } else if world.get::<Checkbox>(entity).is_some() {
            world.insert(
                entity,
                GestureHandler {
                    on_gesture: checkbox_handler,
                },
            );
        } else if world.get::<ProgressBar>(entity).is_some() {
            world.insert(
                entity,
                GestureHandler {
                    on_gesture: progress_bar_handler,
                },
            );
        }
    }
}
