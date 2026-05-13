use crate::ecs::{Entity, World};
use crate::surface::InputEvent;

use super::hit_test::hit_test;
use super::{EventHandler, WidgetEvent};

/// Dispatch an InputEvent: hit test → find handler → invoke callback
pub fn dispatch(world: &World, root: Entity, event: &InputEvent, screen_w: u16, screen_h: u16) {
    let (widget_event, x, y) = match event {
        InputEvent::Touch { x, y } => (WidgetEvent::TouchDown { x: *x, y: *y }, *x, *y),
        InputEvent::Release { x, y } => (WidgetEvent::Click { x: *x, y: *y }, *x, *y),
        _ => return,
    };

    let Some(target) = hit_test(world, root, x, y, screen_w, screen_h) else {
        return;
    };

    if let Some(handler) = world.get::<EventHandler>(target) {
        (handler.on_event)(target, &widget_event);
    }
}
