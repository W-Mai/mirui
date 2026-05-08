use crate::backend::InputEvent;
use crate::components::scroll::ScrollOffset;
use crate::ecs::{Entity, World};
use crate::event::hit_test::hit_test;

/// Scroll drag state resource
pub struct ScrollDragState {
    pub active: bool,
    pub target: Entity,
    pub last_y: i32,
    pub last_x: i32,
}

impl Default for ScrollDragState {
    fn default() -> Self {
        Self {
            active: false,
            target: Entity {
                id: 0,
                generation: 0,
            },
            last_y: 0,
            last_x: 0,
        }
    }
}

pub fn scroll_system(
    world: &mut World,
    root: Entity,
    event: &InputEvent,
    screen_w: u16,
    screen_h: u16,
) {
    match event {
        InputEvent::Touch { x, y } => {
            // Find if touch lands on a scrollable widget
            if let Some(target) = find_scroll_target(world, root, *x, *y, screen_w, screen_h) {
                if let Some(state) = world.resource_mut::<ScrollDragState>() {
                    state.active = true;
                    state.target = target;
                    state.last_x = *x;
                    state.last_y = *y;
                }
            }
        }
        InputEvent::TouchMove { x, y } => {
            let (active, target, last_x, last_y) = {
                let Some(state) = world.resource::<ScrollDragState>() else {
                    return;
                };
                (state.active, state.target, state.last_x, state.last_y)
            };
            if active {
                let dx = *x - last_x;
                let dy = *y - last_y;
                if let Some(scroll) = world.get_mut::<ScrollOffset>(target) {
                    scroll.x -= dx;
                    scroll.y -= dy;
                }
                world.insert(target, crate::widget::dirty::Dirty);
                if let Some(state) = world.resource_mut::<ScrollDragState>() {
                    state.last_x = *x;
                    state.last_y = *y;
                }
            }
        }
        InputEvent::Release { .. } => {
            if let Some(state) = world.resource_mut::<ScrollDragState>() {
                state.active = false;
            }
        }
        _ => {}
    }
}

fn find_scroll_target(
    world: &World,
    root: Entity,
    x: i32,
    y: i32,
    screen_w: u16,
    screen_h: u16,
) -> Option<Entity> {
    // Walk up from hit target to find nearest scrollable ancestor
    let hit = hit_test(world, root, x, y, screen_w, screen_h)?;
    // Check hit entity and its ancestors for ScrollOffset
    let mut current = hit;
    loop {
        if world.get::<ScrollOffset>(current).is_some() {
            return Some(current);
        }
        if let Some(parent) = world.get::<crate::widget::Parent>(current) {
            current = parent.0;
        } else {
            break;
        }
    }
    None
}
