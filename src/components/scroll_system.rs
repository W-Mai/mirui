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

    if active {
        return;
    }

    // Get scroll bounds for elastic
    let (offset_x, offset_y, max_x, max_y, elastic) = {
        let Some(scroll) = world.get::<ScrollOffset>(target) else {
            return;
        };
        let config = world.get::<crate::components::scroll::ScrollConfig>(target);
        let computed = world.get::<crate::widget::ComputedRect>(target);
        let container_h = computed.map(|c| c.0.h as i32).unwrap_or(0);
        let container_w = computed.map(|c| c.0.w as i32).unwrap_or(0);
        let content_h = config
            .map(|c| c.content_height as i32)
            .unwrap_or(container_h);
        let content_w = config
            .map(|c| c.content_width as i32)
            .unwrap_or(container_w);
        let max_y = (content_h - container_h).max(0);
        let max_x = (content_w - container_w).max(0);
        let elastic = config.map(|c| c.elastic).unwrap_or(true);
        (scroll.x, scroll.y, max_x, max_y, elastic)
    };

    let mut new_vel_x = vel_x;
    let mut new_vel_y = vel_y;

    // Elastic bounce — if out of bounds, ignore inertia, lerp back to boundary
    let mut bouncing = false;
    if elastic {
        if offset_y < 0 {
            let diff = 0 - offset_y;
            new_vel_y = if diff.abs() <= 3 { diff } else { diff / 3 };
            new_vel_x = 0;
            bouncing = true;
        } else if offset_y > max_y {
            let diff = max_y - offset_y;
            new_vel_y = if diff.abs() <= 3 { diff } else { diff / 3 };
            new_vel_x = 0;
            bouncing = true;
        }
        if offset_x < 0 {
            let diff = 0 - offset_x;
            new_vel_x = if diff.abs() <= 3 { diff } else { diff / 3 };
            bouncing = true;
        } else if offset_x > max_x {
            let diff = max_x - offset_x;
            new_vel_x = if diff.abs() <= 3 { diff } else { diff / 3 };
            bouncing = true;
        }
    }

    if new_vel_x == 0 && new_vel_y == 0 {
        return;
    }

    // Apply velocity
    if let Some(scroll) = world.get_mut::<ScrollOffset>(target) {
        scroll.x += new_vel_x;
        scroll.y += new_vel_y;
    }
    world.insert(target, crate::widget::dirty::Dirty);

    // Decay velocity (only when not bouncing)
    if let Some(state) = world.resource_mut::<ScrollDragState>() {
        if bouncing {
            state.vel_x = 0;
            state.vel_y = 0;
        } else {
            state.vel_x = new_vel_x * 9 / 10;
            state.vel_y = new_vel_y * 9 / 10;
            if state.vel_x.abs() < 1 {
                state.vel_x = 0;
            }
            if state.vel_y.abs() < 1 {
                state.vel_y = 0;
            }
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
