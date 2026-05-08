pub mod dispatch;
pub mod hit_test;

use alloc::boxed::Box;

use crate::ecs::Entity;
use crate::types::Fixed;

/// Events that widgets can receive
#[derive(Clone, Debug)]
pub enum WidgetEvent {
    Click { x: Fixed, y: Fixed },
    TouchDown { x: Fixed, y: Fixed },
    TouchUp { x: Fixed, y: Fixed },
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
