pub mod dispatch;
pub mod hit_test;

use alloc::boxed::Box;

use crate::ecs::Entity;

/// Events that widgets can receive
#[derive(Clone, Debug)]
pub enum WidgetEvent {
    Click { x: i32, y: i32 },
    TouchDown { x: i32, y: i32 },
    TouchUp { x: i32, y: i32 },
}

type EventCallback = Box<dyn Fn(Entity, &WidgetEvent) + Send>;

/// Component: attach an event handler to a widget entity
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
