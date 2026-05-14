pub mod dispatch;
pub mod focus;
pub mod gesture;
pub mod hit_test;
pub mod input;
pub mod scroll;
pub mod widget_input;

use alloc::boxed::Box;

use crate::ecs::{Entity, World};
use crate::types::Fixed;
use crate::widget::Parent;

use gesture::GestureEvent;

/// Events that widgets can receive (legacy — scheduled for removal)
#[derive(Clone, Debug)]
pub enum WidgetEvent {
    Click { x: Fixed, y: Fixed },
    TouchDown { x: Fixed, y: Fixed },
    TouchUp { x: Fixed, y: Fixed },
}

type EventCallback = Box<dyn Fn(Entity, &WidgetEvent) + Send>;

/// Component: attach an event handler to a widget entity (legacy — scheduled for removal)
pub struct EventHandler {
    pub on_event: EventCallback,
}

impl EventHandler {
    pub fn new(f: impl Fn(Entity, &WidgetEvent) + Send + 'static) -> Self {
        Self {
            on_event: Box::new(f),
        }
    }
}

/// New gesture handler component — a plain fn pointer, no heap allocation.
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
