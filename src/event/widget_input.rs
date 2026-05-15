use crate::components::button::Button;
use crate::components::checkbox::Checkbox;
use crate::components::progress_bar::ProgressBar;
use crate::components::tabbar::TabBar;
use crate::ecs::{Entity, World};
use crate::types::Fixed;
use crate::widget::dirty::Dirty;

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

/// TabBar tap → snap selected, jump indicator_offset. Apps that want
/// a smooth slide attach an animate! component on the same entity and
/// drive `indicator_offset` from a Tween (same pattern Switch uses for
/// its thumb position).
fn tabbar_handler(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    let x = match event {
        GestureEvent::Tap { x, .. } => *x,
        _ => return false,
    };
    let Some(rect) = world
        .get::<crate::widget::ComputedRect>(entity)
        .map(|c| c.0)
    else {
        return false;
    };
    if rect.w <= Fixed::ZERO {
        return false;
    }
    let count = match world.get::<TabBar>(entity) {
        Some(tb) if tb.count > 0 => tb.count,
        _ => return false,
    };
    let local = (x - rect.x).max(Fixed::ZERO);
    let tab_w = rect.w / Fixed::from_int(count as i32);
    let idx = (local / tab_w).to_int().clamp(0, count as i32 - 1) as u8;
    if let Some(tb) = world.get_mut::<TabBar>(entity) {
        tb.selected = idx;
        tb.indicator_offset = Fixed::from_int(idx as i32);
    }
    world.insert(entity, Dirty);
    true
}

/// ProgressBar reads the last rendered rect (PrevRect) so it can map
/// the pointer x to a 0..1 ratio without a layout re-walk.
fn progress_bar_handler(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    let x = match event {
        GestureEvent::Tap { x, .. } | GestureEvent::DragMove { x, .. } => *x,
        _ => return false,
    };
    let Some(rect) = world
        .get::<crate::widget::ComputedRect>(entity)
        .map(|c| c.0)
    else {
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
        } else if world.get::<TabBar>(entity).is_some() {
            world.insert(
                entity,
                GestureHandler {
                    on_gesture: tabbar_handler,
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Rect;
    use crate::widget::ComputedRect;

    #[test]
    fn tabbar_tap_picks_correct_tab() {
        let mut world = World::default();
        let e = world.spawn();
        world.insert(e, ComputedRect(Rect::new(0, 0, 300, 40)));
        world.insert(e, TabBar::new(3));
        // Tab width = 100. Tap at x=50 → tab 0; x=150 → tab 1; x=270 → tab 2.
        for (x, expected) in [(50, 0u8), (150, 1), (270, 2), (0, 0), (299, 2)] {
            tabbar_handler(
                &mut world,
                e,
                &GestureEvent::Tap {
                    x: Fixed::from_int(x),
                    y: Fixed::from_int(20),
                    target: e,
                },
            );
            let tb = world.get::<TabBar>(e).unwrap();
            assert_eq!(tb.selected, expected, "x={x} → expected {expected}");
            assert_eq!(tb.indicator_offset, Fixed::from_int(expected as i32));
        }
    }

    #[test]
    fn tabbar_ignores_non_tap() {
        let mut world = World::default();
        let e = world.spawn();
        world.insert(e, ComputedRect(Rect::new(0, 0, 300, 40)));
        world.insert(e, TabBar::new(3));
        let consumed = tabbar_handler(
            &mut world,
            e,
            &GestureEvent::DragMove {
                x: Fixed::from_int(50),
                y: Fixed::from_int(20),
                dx: Fixed::ZERO,
                dy: Fixed::ZERO,
                target: e,
            },
        );
        assert!(!consumed);
        assert_eq!(world.get::<TabBar>(e).unwrap().selected, 0);
    }
}
