use crate::backend::InputEvent;
use crate::components::scroll::{ScrollAxis, ScrollConfig, ScrollOffset};
use crate::ecs::{Entity, World};
use crate::event::hit_test::hit_test;

/// Scroll drag state resource
pub struct ScrollDragState {
    pub active: bool,
    pub resolved: bool,
    pub target: Entity,
    pub hit_entity: Entity,
    pub start_x: i32,
    pub start_y: i32,
    pub last_x: i32,
    pub last_y: i32,
    pub vel_x: i32,
    pub vel_y: i32,
}

impl Default for ScrollDragState {
    fn default() -> Self {
        let null = Entity {
            id: 0,
            generation: 0,
        };
        Self {
            active: false,
            resolved: false,
            target: null,
            hit_entity: null,
            start_x: 0,
            start_y: 0,
            last_x: 0,
            last_y: 0,
            vel_x: 0,
            vel_y: 0,
        }
    }
}

const DIRECTION_THRESHOLD: i32 = 5;

pub fn scroll_system(
    world: &mut World,
    root: Entity,
    event: &InputEvent,
    screen_w: u16,
    screen_h: u16,
) {
    match event {
        InputEvent::Touch { x, y } => {
            // Record start, don't resolve target yet
            let hit = hit_test(world, root, *x, *y, screen_w, screen_h);
            if let Some(state) = world.resource_mut::<ScrollDragState>() {
                state.active = true;
                state.resolved = false;
                state.hit_entity = hit.unwrap_or(Entity {
                    id: 0,
                    generation: 0,
                });
                state.start_x = *x;
                state.start_y = *y;
                state.last_x = *x;
                state.last_y = *y;
                state.vel_x = 0;
                state.vel_y = 0;
            }
        }
        InputEvent::TouchMove { x, y } => {
            let (active, resolved, _target, hit_entity, last_x, last_y, start_x, start_y) = {
                let Some(state) = world.resource::<ScrollDragState>() else {
                    return;
                };
                (
                    state.active,
                    state.resolved,
                    state.target,
                    state.hit_entity,
                    state.last_x,
                    state.last_y,
                    state.start_x,
                    state.start_y,
                )
            };

            if !active {
                return;
            }

            // If not resolved yet, check if we've moved enough to determine direction
            if !resolved {
                let total_dx = (*x - start_x).abs();
                let total_dy = (*y - start_y).abs();

                if total_dx < DIRECTION_THRESHOLD && total_dy < DIRECTION_THRESHOLD {
                    // Not enough movement to determine direction
                    if let Some(state) = world.resource_mut::<ScrollDragState>() {
                        state.last_x = *x;
                        state.last_y = *y;
                    }
                    return;
                }

                // Determine gesture direction
                let gesture_dir = if total_dy > total_dx {
                    ScrollAxis::Vertical
                } else {
                    ScrollAxis::Horizontal
                };

                // Find scroll target matching this direction
                let found = find_scroll_target_for_direction(world, hit_entity, gesture_dir);

                if let Some(state) = world.resource_mut::<ScrollDragState>() {
                    state.resolved = true;
                    if let Some(t) = found {
                        state.target = t;
                    } else {
                        // No matching scroll target — deactivate
                        state.active = false;
                        return;
                    }
                }
            }

            // Apply scroll delta
            let (target,) = {
                let Some(state) = world.resource::<ScrollDragState>() else {
                    return;
                };
                (state.target,)
            };

            let dx = *x - last_x;
            let dy = *y - last_y;

            // Only apply delta in the scroll direction
            let config = world.get::<ScrollConfig>(target);
            let dir = config.map(|c| c.direction).unwrap_or(ScrollAxis::Vertical);

            if let Some(scroll) = world.get_mut::<ScrollOffset>(target) {
                match dir {
                    ScrollAxis::Vertical => scroll.y -= dy,
                    ScrollAxis::Horizontal => scroll.x -= dx,
                    ScrollAxis::Both => {
                        scroll.x -= dx;
                        scroll.y -= dy;
                    }
                }
            }
            world.insert(target, crate::widget::dirty::Dirty);

            if let Some(state) = world.resource_mut::<ScrollDragState>() {
                match dir {
                    ScrollAxis::Vertical => {
                        state.vel_x = 0;
                        state.vel_y = -dy;
                    }
                    ScrollAxis::Horizontal => {
                        state.vel_x = -dx;
                        state.vel_y = 0;
                    }
                    ScrollAxis::Both => {
                        state.vel_x = -dx;
                        state.vel_y = -dy;
                    }
                }
                state.last_x = *x;
                state.last_y = *y;
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

/// Inertia system — call every frame to decelerate scroll after release
pub fn scroll_inertia_system(world: &mut World) {
    let (active, resolved, target, vel_x, vel_y) = {
        let Some(state) = world.resource::<ScrollDragState>() else {
            return;
        };
        (
            state.active,
            state.resolved,
            state.target,
            state.vel_x,
            state.vel_y,
        )
    };

    if active || !resolved {
        return;
    }

    // Get scroll bounds for elastic
    let (offset_x, offset_y, max_x, max_y, elastic, dir) = {
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
        let dir = config.map(|c| c.direction).unwrap_or(ScrollAxis::Vertical);
        (scroll.x, scroll.y, max_x, max_y, elastic, dir)
    };

    let mut new_vel_x = vel_x;
    let mut new_vel_y = vel_y;

    // Elastic bounce
    let mut bouncing = false;
    if elastic {
        match dir {
            ScrollAxis::Vertical | ScrollAxis::Both => {
                if offset_y < 0 {
                    let diff = 0 - offset_y;
                    new_vel_y = if diff.abs() <= 3 { diff } else { diff / 3 };
                    bouncing = true;
                } else if offset_y > max_y {
                    let diff = max_y - offset_y;
                    new_vel_y = if diff.abs() <= 3 { diff } else { diff / 3 };
                    bouncing = true;
                }
            }
            _ => {}
        }
        match dir {
            ScrollAxis::Horizontal | ScrollAxis::Both => {
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
            _ => {}
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

    // Decay velocity
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

/// Find a scroll target that matches the gesture direction.
/// Walks up from `start` entity through parents.
fn find_scroll_target_for_direction(
    world: &World,
    start: Entity,
    gesture_dir: ScrollAxis,
) -> Option<Entity> {
    let mut current = start;
    loop {
        if world.get::<ScrollOffset>(current).is_some() {
            let config = world.get::<ScrollConfig>(current);
            let scroll_dir = config.map(|c| c.direction).unwrap_or(ScrollAxis::Vertical);

            let dir_matches = matches!(
                (scroll_dir, gesture_dir),
                (ScrollAxis::Both, _)
                    | (ScrollAxis::Vertical, ScrollAxis::Vertical)
                    | (ScrollAxis::Horizontal, ScrollAxis::Horizontal)
            );

            if dir_matches {
                return Some(current);
            }
            // Direction doesn't match — continue up
        }
        if let Some(parent) = world.get::<crate::widget::Parent>(current) {
            current = parent.0;
        } else {
            break;
        }
    }
    None
}
