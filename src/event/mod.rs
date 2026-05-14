pub mod focus;
pub mod gesture;
pub mod hit_test;
pub mod input;
pub mod scroll;
pub mod widget_input;

use crate::ecs::{Entity, World};
use crate::widget::Parent;

use gesture::GestureEvent;

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
