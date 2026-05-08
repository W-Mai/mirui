use crate::backend::InputEvent;
use crate::components::scroll::{ScrollAxis, ScrollConfig, ScrollOffset};
use crate::ecs::{Entity, World};
use crate::event::hit_test::hit_test;
use crate::types::Fixed;

/// Scroll drag state resource
pub struct ScrollDragState {
    pub active: bool,
    pub resolved: bool,
    pub target: Entity,
    pub hit_entity: Entity,
    pub start_x: Fixed,
    pub start_y: Fixed,
    pub last_x: Fixed,
    pub last_y: Fixed,
    pub vel_x: Fixed,
    pub vel_y: Fixed,
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
            start_x: Fixed::ZERO,
            start_y: Fixed::ZERO,
            last_x: Fixed::ZERO,
            last_y: Fixed::ZERO,
            vel_x: Fixed::ZERO,
            vel_y: Fixed::ZERO,
        }
    }
}

const DIRECTION_THRESHOLD: Fixed = Fixed::from_int(5);

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
                state.vel_x = Fixed::ZERO;
                state.vel_y = Fixed::ZERO;
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
                // delta sign: positive = user dragging content up (wants to scroll down)
                let scroll_dx = -(*x - start_x);
                let scroll_dy = -(*y - start_y);
                let found = find_scroll_target_for_direction(
                    world,
                    hit_entity,
                    gesture_dir,
                    scroll_dx,
                    scroll_dy,
                );

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

            // Apply scroll delta — no mid-drag chaining (iOS behavior)
            // Chain decision was made at resolve time. During drag, always apply to target.
            let (target,) = {
                let Some(state) = world.resource::<ScrollDragState>() else {
                    return;
                };
                (state.target,)
            };

            let dx = *x - last_x;
            let dy = *y - last_y;

            // Read bounds for resistance calculation
            let config = world.get::<ScrollConfig>(target);
            let dir = config.map(|c| c.direction).unwrap_or(ScrollAxis::Vertical);
            let computed = world.get::<crate::widget::ComputedRect>(target);
            let container_h = computed.map(|c| c.0.h.to_int()).unwrap_or(0);
            let container_w = computed.map(|c| c.0.w.to_int()).unwrap_or(0);
            let content_h = config
                .map(|c| c.content_height as i32)
                .unwrap_or(container_h);
            let content_w = config
                .map(|c| c.content_width as i32)
                .unwrap_or(container_w);
            let max_y = (content_h - container_h).max(0);
            let max_x = (content_w - container_w).max(0);

            if let Some(scroll) = world.get_mut::<ScrollOffset>(target) {
                match dir {
                    ScrollAxis::Vertical => {
                        let eff_dy = elastic_resist(scroll.y, -dy, Fixed::from_int(max_y));
                        scroll.y += eff_dy;
                    }
                    ScrollAxis::Horizontal => {
                        let eff_dx = elastic_resist(scroll.x, -dx, Fixed::from_int(max_x));
                        scroll.x += eff_dx;
                    }
                    ScrollAxis::Both => {
                        let eff_dx = elastic_resist(scroll.x, -dx, Fixed::from_int(max_x));
                        let eff_dy = elastic_resist(scroll.y, -dy, Fixed::from_int(max_y));
                        scroll.x += eff_dx;
                        scroll.y += eff_dy;
                    }
                }
            }
            world.insert(target, crate::widget::dirty::Dirty);

            let dir = world
                .get::<ScrollConfig>(target)
                .map(|c| c.direction)
                .unwrap_or(ScrollAxis::Vertical);
            if let Some(state) = world.resource_mut::<ScrollDragState>() {
                match dir {
                    ScrollAxis::Vertical => {
                        state.vel_x = Fixed::ZERO;
                        state.vel_y = -dy;
                    }
                    ScrollAxis::Horizontal => {
                        state.vel_x = -dx;
                        state.vel_y = Fixed::ZERO;
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
        let container_h = computed.map(|c| c.0.h.to_int()).unwrap_or(0);
        let container_w = computed.map(|c| c.0.w.to_int()).unwrap_or(0);
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
        (
            scroll.x.to_int(),
            scroll.y.to_int(),
            max_x,
            max_y,
            elastic,
            dir,
        )
    };

    let mut new_vel_x = vel_x;
    let mut new_vel_y = vel_y;

    // Elastic bounce
    let mut bouncing = false;
    if elastic {
        match dir {
            ScrollAxis::Vertical | ScrollAxis::Both => {
                if offset_y < 0 {
                    let diff = Fixed::from_int(-offset_y);
                    new_vel_y = if diff.abs() <= Fixed::from_int(3) {
                        diff
                    } else {
                        diff / 3
                    };
                    bouncing = true;
                } else if offset_y > max_y {
                    let diff = Fixed::from_int(max_y - offset_y);
                    new_vel_y = if diff.abs() <= Fixed::from_int(3) {
                        diff
                    } else {
                        diff / 3
                    };
                    bouncing = true;
                }
            }
            _ => {}
        }
        match dir {
            ScrollAxis::Horizontal | ScrollAxis::Both => {
                if offset_x < 0 {
                    let diff = Fixed::from_int(-offset_x);
                    new_vel_x = if diff.abs() <= Fixed::from_int(3) {
                        diff
                    } else {
                        diff / 3
                    };
                    bouncing = true;
                } else if offset_x > max_x {
                    let diff = Fixed::from_int(max_x - offset_x);
                    new_vel_x = if diff.abs() <= Fixed::from_int(3) {
                        diff
                    } else {
                        diff / 3
                    };
                    bouncing = true;
                }
            }
            _ => {}
        }
    }

    if new_vel_x == Fixed::ZERO && new_vel_y == Fixed::ZERO {
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
            state.vel_x = Fixed::ZERO;
            state.vel_y = Fixed::ZERO;
        } else {
            state.vel_x = new_vel_x * 9 / 10;
            state.vel_y = new_vel_y * 9 / 10;
            if state.vel_x.abs() < Fixed::ONE {
                state.vel_x = Fixed::ZERO;
            }
            if state.vel_y.abs() < Fixed::ONE {
                state.vel_y = Fixed::ZERO;
            }
        }
    }
}

fn find_scroll_target_for_direction(
    world: &World,
    start: Entity,
    gesture_dir: ScrollAxis,
    delta_x: Fixed,
    delta_y: Fixed,
) -> Option<Entity> {
    let mut current = start;
    let mut last_matching: Option<Entity> = None;
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
                last_matching = Some(current);
                let at_boundary = is_at_boundary(world, current, delta_x, delta_y);
                if !at_boundary {
                    return Some(current);
                }
            }
        }
        if let Some(parent) = world.get::<crate::widget::Parent>(current) {
            current = parent.0;
        } else {
            break;
        }
    }
    // All matching scrolls are at boundary — return outermost for elastic
    last_matching
}

/// Apply elastic resistance when overscrolling.
/// `offset`: current scroll offset, `delta`: requested change, `max`: max allowed offset.
/// Returns the effective delta to apply (reduced when out of bounds).
fn elastic_resist(offset: Fixed, delta: Fixed, max: Fixed) -> Fixed {
    const DAMPING: i32 = 200;

    let new_offset = offset + delta;

    // If staying in bounds, no resistance
    if new_offset >= Fixed::ZERO && new_offset <= max {
        return delta;
    }

    // Calculate how far out of bounds we are (or will be)
    let overscroll = if new_offset < Fixed::ZERO {
        -new_offset
    } else {
        new_offset - max
    };

    // Resistance: damping / (damping + overscroll)
    // Use integer math on raw values
    let over_int = overscroll.to_int();
    let resistance_denom = DAMPING + over_int;
    if resistance_denom == 0 {
        return Fixed::ZERO;
    }
    delta * DAMPING / resistance_denom
}
fn is_at_boundary(world: &World, entity: Entity, delta_x: Fixed, delta_y: Fixed) -> bool {
    let Some(scroll) = world.get::<ScrollOffset>(entity) else {
        return false;
    };
    let config = world.get::<ScrollConfig>(entity);
    let computed = world.get::<crate::widget::ComputedRect>(entity);
    let container_h = computed.map(|c| c.0.h.to_int()).unwrap_or(0);
    let container_w = computed.map(|c| c.0.w.to_int()).unwrap_or(0);
    let content_h = config
        .map(|c| c.content_height as i32)
        .unwrap_or(container_h);
    let content_w = config
        .map(|c| c.content_width as i32)
        .unwrap_or(container_w);
    let max_y = Fixed::from_int((content_h - container_h).max(0));
    let max_x = Fixed::from_int((content_w - container_w).max(0));

    let at_y = (scroll.y <= Fixed::ZERO && delta_y < Fixed::ZERO)
        || (scroll.y >= max_y && delta_y > Fixed::ZERO);
    let at_x = (scroll.x <= Fixed::ZERO && delta_x < Fixed::ZERO)
        || (scroll.x >= max_x && delta_x > Fixed::ZERO);

    let dir = config.map(|c| c.direction).unwrap_or(ScrollAxis::Vertical);
    match dir {
        ScrollAxis::Vertical => at_y,
        ScrollAxis::Horizontal => at_x,
        ScrollAxis::Both => at_y || at_x,
    }
}
