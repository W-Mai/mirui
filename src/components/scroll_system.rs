use crate::backend::InputEvent;
use crate::components::scroll::ScrollOffset;
use crate::ecs::{Entity, World};
use crate::event::hit_test::hit_test;

/// Scroll drag state resource
pub struct ScrollDragState {
    pub active: bool,
    pub target: Entity,
    pub last_x: i32,
    pub last_y: i32,
    pub vel_x: i32,
    pub vel_y: i32,
}

impl Default for ScrollDragState {
    fn default() -> Self {
        Self {
            active: false,
            target: Entity {
                id: 0,
                generation: 0,
            },
            last_x: 0,
            last_y: 0,
            vel_x: 0,
            vel_y: 0,
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
                    state.vel_x = -dx;
                    state.vel_y = -dy;
                    state.last_x = *x;
                    state.last_y = *y;
                }
            }
        }
        InputEvent::Release { .. } => {
            if let Some(state) = world.resource_mut::<ScrollDragState>() {
                state.active = false;
                // velocity preserved for inertia
            }
        }
        _ => {}
    }
}

/// Inertia system — call every frame to decelerate scroll after release
pub fn scroll_inertia_system(world: &mut World) {
    let (active, target, vel_x, vel_y) = {
        let Some(state) = world.resource::<ScrollDragState>() else {
            return;
        };
        (state.active, state.target, state.vel_x, state.vel_y)
    };

    if active || (vel_x == 0 && vel_y == 0) {
        return;
    }

    // Apply velocity
    if let Some(scroll) = world.get_mut::<ScrollOffset>(target) {
        scroll.x += vel_x;
        scroll.y += vel_y;
    }
    world.insert(target, crate::widget::dirty::Dirty);

    // Decay velocity (friction)
    if let Some(state) = world.resource_mut::<ScrollDragState>() {
        state.vel_x = state.vel_x * 9 / 10;
        state.vel_y = state.vel_y * 9 / 10;
        // Stop when slow enough
        if state.vel_x.abs() < 1 {
            state.vel_x = 0;
        }
        if state.vel_y.abs() < 1 {
            state.vel_y = 0;
        }
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
