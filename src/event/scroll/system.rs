use super::components::{ScrollAxis, ScrollConfig, ScrollOffset};
use crate::anim::{BOUNCY, SMOOTH, Spring};
use crate::ecs::{Entity, World};
use crate::event::hit_test::hit_test;
use crate::event::input::InputEvent;
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
    pub last_resolved_target: Option<Entity>,
}

#[derive(Default)]
pub struct ScrollSpring {
    pub x: Option<Spring>,
    pub y: Option<Spring>,
    pub target_entity: Option<Entity>,
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
            last_resolved_target: None,
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
        InputEvent::PointerDown { x, y, .. } => {
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
            if let Some(ss) = world.resource_mut::<ScrollSpring>() {
                ss.x = None;
                ss.y = None;
                ss.target_entity = None;
            }
        }
        InputEvent::PointerMove { x, y, .. } => {
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
                        state.last_resolved_target = Some(t);
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
            let container_h = computed.map(|c| c.0.h).unwrap_or(Fixed::ZERO);
            let container_w = computed.map(|c| c.0.w).unwrap_or(Fixed::ZERO);
            let content_h: Fixed = config.map(|c| c.content_height).unwrap_or(container_h);
            let content_w: Fixed = config.map(|c| c.content_width).unwrap_or(container_w);
            let max_y = (content_h - container_h).max(Fixed::ZERO);
            let max_x = (content_w - container_w).max(Fixed::ZERO);

            if let Some(scroll) = world.get_mut::<ScrollOffset>(target) {
                match dir {
                    ScrollAxis::Vertical => {
                        let eff_dy = elastic_resist(scroll.y, -dy, max_y);
                        scroll.y += eff_dy;
                    }
                    ScrollAxis::Horizontal => {
                        let eff_dx = elastic_resist(scroll.x, -dx, max_x);
                        scroll.x += eff_dx;
                    }
                    ScrollAxis::Both => {
                        let eff_dx = elastic_resist(scroll.x, -dx, max_x);
                        let eff_dy = elastic_resist(scroll.y, -dy, max_y);
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
        InputEvent::PointerUp { .. } => {
            let (vel_x, vel_y, target_entity, dir) = {
                let Some(state) = world.resource::<ScrollDragState>() else {
                    return;
                };
                if !state.resolved {
                    if let Some(state) = world.resource_mut::<ScrollDragState>() {
                        state.active = false;
                    }
                    return;
                }
                let t = state.target;
                let dir = world
                    .get::<ScrollConfig>(t)
                    .map(|c| c.direction)
                    .unwrap_or(ScrollAxis::Vertical);
                (state.vel_x, state.vel_y, t, dir)
            };

            let (offset_x, offset_y) = world
                .get::<ScrollOffset>(target_entity)
                .map(|s| (s.x, s.y))
                .unwrap_or((Fixed::ZERO, Fixed::ZERO));

            let spring_x = match dir {
                ScrollAxis::Horizontal | ScrollAxis::Both => {
                    let projected = offset_x + vel_x * Fixed::from_int(20);
                    Some(Spring::preset(offset_x, projected, SMOOTH).with_velocity(vel_x))
                }
                _ => None,
            };
            let spring_y = match dir {
                ScrollAxis::Vertical | ScrollAxis::Both => {
                    let projected = offset_y + vel_y * Fixed::from_int(20);
                    Some(Spring::preset(offset_y, projected, SMOOTH).with_velocity(vel_y))
                }
                _ => None,
            };

            if let Some(ss) = world.resource_mut::<ScrollSpring>() {
                ss.x = spring_x;
                ss.y = spring_y;
                ss.target_entity = Some(target_entity);
            }

            if let Some(state) = world.resource_mut::<ScrollDragState>() {
                state.active = false;
            }
        }
        InputEvent::Rotary { delta, .. } => {
            let target = world
                .resource::<ScrollDragState>()
                .and_then(|s| s.last_resolved_target);
            if let Some(target) = target {
                let step = Fixed::from_int(20);
                let offset = Fixed::from(*delta as i32) * step;
                let axis = world
                    .get::<ScrollConfig>(target)
                    .map(|c| c.direction)
                    .unwrap_or(ScrollAxis::Vertical);
                if let Some(scroll) = world.get_mut::<ScrollOffset>(target) {
                    match axis {
                        ScrollAxis::Vertical => scroll.y -= offset,
                        ScrollAxis::Horizontal => scroll.x -= offset,
                        ScrollAxis::Both => scroll.y -= offset,
                    }
                }
                world.insert(target, crate::widget::dirty::Dirty);
            }
        }
        _ => {}
    }
}

/// Inertia system — call every frame to decelerate scroll after release
pub fn scroll_inertia_system(world: &mut World) {
    let active = {
        let Some(state) = world.resource::<ScrollDragState>() else {
            return;
        };
        state.active
    };

    if active {
        return;
    }

    let (has_spring, target_entity) = {
        let Some(ss) = world.resource::<ScrollSpring>() else {
            return;
        };
        (ss.x.is_some() || ss.y.is_some(), ss.target_entity)
    };

    if !has_spring {
        return;
    }

    let Some(target) = target_entity else {
        return;
    };

    let dt = world
        .resource::<crate::ecs::DeltaTimeMs>()
        .map_or(16, |r| r.0);

    let (max_x, max_y, elastic) = {
        let config = world.get::<ScrollConfig>(target);
        let computed = world.get::<crate::widget::ComputedRect>(target);
        let container_h = computed.map(|c| c.0.h).unwrap_or(Fixed::ZERO);
        let container_w = computed.map(|c| c.0.w).unwrap_or(Fixed::ZERO);
        let content_h = config.map(|c| c.content_height).unwrap_or(container_h);
        let content_w = config.map(|c| c.content_width).unwrap_or(container_w);
        let max_y = (content_h - container_h).max(Fixed::ZERO);
        let max_x = (content_w - container_w).max(Fixed::ZERO);
        let elastic = config.map(|c| c.elastic).unwrap_or(true);
        (max_x, max_y, elastic)
    };

    let (new_x, new_y, done) = {
        let Some(ss) = world.resource_mut::<ScrollSpring>() else {
            return;
        };

        let mut done = true;

        if let Some(ref mut sx) = ss.x {
            sx.tick(dt);
            let pos = sx.value();
            if elastic {
                if pos < Fixed::ZERO {
                    sx.retarget(Fixed::ZERO, Some(BOUNCY));
                } else if pos > max_x {
                    sx.retarget(max_x, Some(BOUNCY));
                }
            }
            if !sx.is_settled() {
                done = false;
            }
        }

        if let Some(ref mut sy) = ss.y {
            sy.tick(dt);
            let pos = sy.value();
            if elastic {
                if pos < Fixed::ZERO {
                    sy.retarget(Fixed::ZERO, Some(BOUNCY));
                } else if pos > max_y {
                    sy.retarget(max_y, Some(BOUNCY));
                }
            }
            if !sy.is_settled() {
                done = false;
            }
        }

        let nx = ss.x.as_ref().map(|s| s.value());
        let ny = ss.y.as_ref().map(|s| s.value());
        (nx, ny, done)
    };

    let mut changed = false;
    if let Some(scroll) = world.get_mut::<ScrollOffset>(target) {
        if let Some(nx) = new_x {
            if (nx - scroll.x).abs() >= Fixed::ONE {
                changed = true;
            }
            scroll.x = nx;
        }
        if let Some(ny) = new_y {
            if (ny - scroll.y).abs() >= Fixed::ONE {
                changed = true;
            }
            scroll.y = ny;
        }
    }
    if changed {
        world.insert(target, crate::widget::dirty::Dirty);
    }

    if done {
        if let Some(ss) = world.resource_mut::<ScrollSpring>() {
            ss.x = None;
            ss.y = None;
            ss.target_entity = None;
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
    let container_h = computed.map(|c| c.0.h).unwrap_or(Fixed::ZERO);
    let container_w = computed.map(|c| c.0.w).unwrap_or(Fixed::ZERO);
    let content_h = config.map(|c| c.content_height).unwrap_or(container_h);
    let content_w = config.map(|c| c.content_width).unwrap_or(container_w);
    let max_y = (content_h - container_h).max(Fixed::ZERO);
    let max_x = (content_w - container_w).max(Fixed::ZERO);

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
