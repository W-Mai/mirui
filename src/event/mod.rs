pub mod focus;
pub mod gesture;
pub mod hit_test;
pub mod input;
pub mod scroll;
pub mod sim;
pub mod widget_input;

use crate::ecs::{Entity, World};
use crate::types::Fixed;
use crate::widget::{Parent, UserState};

use focus::key_dispatch;
use gesture::{GestureEvent, GestureSystem};
use hit_test::hit_test;
use input::InputEvent;
use scroll::{ScrollDragState, scroll_system};

#[derive(Clone, Copy, Default)]
pub struct PointerCursor {
    pub x: Fixed,
    pub y: Fixed,
    pub down: bool,
    /// Bumps on every PointerDown / PointerUp; PointerMove leaves it.
    pub event_seq: u32,
}

/// Single source of truth for the per-event side of the input
/// pipeline. Both `App::run`'s real input loop and
/// `sim_timeline_system` (which fakes pointer events) call this so
/// that simulated inputs traverse the exact same scroll / hit-test /
/// gesture-recognizer / key-dispatch path as real ones.
///
/// Does *not* drain `GestureSystem.events` — the caller decides when
/// to dispatch (real input loop drains after the whole `poll_event`
/// burst; sim drains every system tick).
pub fn dispatch_input(
    world: &mut World,
    root: Entity,
    event: &InputEvent,
    now_ms: u32,
    lw: u16,
    lh: u16,
) {
    match event {
        InputEvent::PointerDown { x, y, .. } => {
            let mut next = world
                .resource::<PointerCursor>()
                .copied()
                .unwrap_or_default();
            next.x = *x;
            next.y = *y;
            next.down = true;
            next.event_seq = next.event_seq.wrapping_add(1);
            world.insert_resource(next);
        }
        InputEvent::PointerMove { x, y, .. } => {
            let mut next = world
                .resource::<PointerCursor>()
                .copied()
                .unwrap_or_default();
            next.x = *x;
            next.y = *y;
            world.insert_resource(next);
        }
        InputEvent::PointerUp { x, y, .. } => {
            let mut next = world
                .resource::<PointerCursor>()
                .copied()
                .unwrap_or_default();
            next.x = *x;
            next.y = *y;
            next.down = false;
            next.event_seq = next.event_seq.wrapping_add(1);
            world.insert_resource(next);
        }
        _ => {}
    }

    if let InputEvent::PointerDown { x, y, .. } = event {
        if let Some(target) = hit_test(world, root, *x, *y, lw, lh) {
            if entity_or_ancestor_disabled(world, target) {
                key_dispatch(world, event);
                return;
            }
        }
    }

    scroll_system(world, root, event, lw, lh);

    let hit = match event {
        InputEvent::PointerDown { x, y, .. } => hit_test(world, root, *x, *y, lw, lh),
        _ => None,
    };
    let scroll_claimed = world
        .resource::<ScrollDragState>()
        .is_some_and(|s| s.active && s.resolved);
    if let Some(gs) = world.resource_mut::<GestureSystem>() {
        gs.recognizer.scroll_claimed = scroll_claimed;
        gs.recognizer.update(event, now_ms, hit, &mut gs.events);
    }

    key_dispatch(world, event);
}

pub fn entity_or_ancestor_disabled(world: &World, entity: Entity) -> bool {
    let mut cur = Some(entity);
    while let Some(e) = cur {
        if matches!(world.get::<UserState>(e), Some(UserState::Disabled)) {
            return true;
        }
        cur = world.get::<Parent>(e).map(|p| p.0);
    }
    false
}

/// Gesture handler component — a plain fn pointer, no heap allocation.
/// Returns `true` to stop propagation (event consumed).
pub struct GestureHandler {
    pub on_gesture: fn(&mut World, Entity, &GestureEvent) -> bool,
}

/// Walk from `target` up via `Parent` links, invoking the first
/// `GestureHandler` found. Stops when a handler returns `true`
/// (consumed) or the root is reached.
pub fn bubble_dispatch(world: &mut World, event: &GestureEvent) {
    let mut current = event.target();
    loop {
        let handler_fn = world.get::<GestureHandler>(current).map(|h| h.on_gesture);
        if let Some(f) = handler_fn {
            if f(world, current, event) {
                return;
            }
        }
        match world.get::<Parent>(current) {
            Some(p) => current = p.0,
            None => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ancestor_disabled_propagates() {
        let mut world = World::new();
        let parent = world.spawn();
        let child = world.spawn();
        world.insert(child, Parent(parent));
        world.insert(parent, UserState::Disabled);
        assert!(entity_or_ancestor_disabled(&world, child));
    }

    #[test]
    fn entity_self_disabled() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, UserState::Disabled);
        assert!(entity_or_ancestor_disabled(&world, e));
    }

    #[test]
    fn unrelated_entity_not_disabled() {
        let mut world = World::new();
        let a = world.spawn();
        let b = world.spawn();
        world.insert(a, UserState::Disabled);
        assert!(!entity_or_ancestor_disabled(&world, b));
    }

    #[test]
    fn errored_does_not_propagate_via_disabled_walk() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, UserState::Errored);
        assert!(!entity_or_ancestor_disabled(&world, e));
    }
}
