use crate::ecs::{Entity, World};
use crate::event::gesture::GestureEvent;

pub const MULTI_TAP_WINDOW_MS: u32 = 300;

#[derive(Clone, Copy, Debug)]
pub struct MultiTapEntry {
    pub entity: Entity,
    pub time_ms: u32,
    pub count: u8,
}

#[derive(Default, Debug, Clone, Copy)]
pub struct MultiTapTracker {
    pub last: Option<MultiTapEntry>,
}

impl MultiTapTracker {
    pub fn new() -> Self {
        Self::default()
    }
}

pub(crate) fn observe_gesture(world: &mut World, event: &GestureEvent, now_ms: u32) {
    let GestureEvent::Tap { target, .. } = event else {
        return;
    };
    let target = *target;
    if world.resource::<MultiTapTracker>().is_none() {
        world.insert_resource(MultiTapTracker::new());
    }
    let next_entry = {
        let tracker = world.resource::<MultiTapTracker>().unwrap();
        let count = match tracker.last {
            Some(prev)
                if prev.entity == target
                    && now_ms != 0
                    && prev.time_ms != 0
                    && now_ms.wrapping_sub(prev.time_ms) <= MULTI_TAP_WINDOW_MS =>
            {
                prev.count.saturating_add(1)
            }
            _ => 1,
        };
        MultiTapEntry {
            entity: target,
            time_ms: now_ms,
            count,
        }
    };
    if let Some(t) = world.resource_mut::<MultiTapTracker>() {
        t.last = Some(next_entry);
    }
}

pub fn current_count(world: &World, entity: Entity) -> u8 {
    world
        .resource::<MultiTapTracker>()
        .and_then(|t| t.last.as_ref())
        .filter(|e| e.entity == entity)
        .map(|e| e.count)
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Fixed;

    fn tap(entity: Entity) -> GestureEvent {
        GestureEvent::Tap {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target: entity,
        }
    }

    #[test]
    fn first_tap_records_count_one() {
        let mut world = World::default();
        world.insert_resource(MultiTapTracker::new());
        let e = world.spawn();
        observe_gesture(&mut world, &tap(e), 100);
        assert_eq!(current_count(&world, e), 1);
    }

    #[test]
    fn within_window_increments() {
        let mut world = World::default();
        world.insert_resource(MultiTapTracker::new());
        let e = world.spawn();
        observe_gesture(&mut world, &tap(e), 100);
        observe_gesture(&mut world, &tap(e), 200);
        observe_gesture(&mut world, &tap(e), 350);
        assert_eq!(current_count(&world, e), 3);
    }

    #[test]
    fn outside_window_resets() {
        let mut world = World::default();
        world.insert_resource(MultiTapTracker::new());
        let e = world.spawn();
        observe_gesture(&mut world, &tap(e), 100);
        observe_gesture(&mut world, &tap(e), 500);
        assert_eq!(current_count(&world, e), 1);
    }

    #[test]
    fn different_target_resets() {
        let mut world = World::default();
        world.insert_resource(MultiTapTracker::new());
        let a = world.spawn();
        let b = world.spawn();
        observe_gesture(&mut world, &tap(a), 100);
        observe_gesture(&mut world, &tap(b), 200);
        assert_eq!(current_count(&world, a), 1);
        assert_eq!(current_count(&world, b), 1);
    }

    #[test]
    fn current_count_unrelated_entity_defaults_one() {
        let mut world = World::default();
        world.insert_resource(MultiTapTracker::new());
        let a = world.spawn();
        let b = world.spawn();
        observe_gesture(&mut world, &tap(a), 100);
        assert_eq!(current_count(&world, b), 1);
    }

    #[test]
    fn current_count_missing_resource_returns_one() {
        let world = World::default();
        let e = Entity {
            id: 0,
            generation: 0,
        };
        assert_eq!(current_count(&world, e), 1);
    }

    #[test]
    fn now_ms_zero_never_accumulates() {
        let mut world = World::default();
        world.insert_resource(MultiTapTracker::new());
        let e = world.spawn();
        observe_gesture(&mut world, &tap(e), 0);
        observe_gesture(&mut world, &tap(e), 0);
        observe_gesture(&mut world, &tap(e), 0);
        assert_eq!(current_count(&world, e), 1);
    }

    #[test]
    fn non_tap_event_ignored() {
        let mut world = World::default();
        world.insert_resource(MultiTapTracker::new());
        let e = world.spawn();
        let drag = GestureEvent::DragStart {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target: e,
        };
        observe_gesture(&mut world, &drag, 100);
        assert!(world.resource::<MultiTapTracker>().unwrap().last.is_none());
    }
}
