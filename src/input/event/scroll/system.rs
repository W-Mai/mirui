use super::components::{ScrollAxis, ScrollConfig, ScrollDelta, ScrollOffset, TouchAction};
use crate::anim::{BOUNCY, SMOOTH, Spring};
use crate::ecs::{Entity, World};
use crate::input::event::hit_test::hit_test;
use crate::input::event::input::InputEvent;
use crate::types::{Fixed, Fixed64};

fn descendant_extent(world: &World, entity: Entity) -> Option<(Fixed, Fixed)> {
    let rect = world.get::<crate::ui::ComputedRect>(entity)?.0;
    let mut right = rect.x + rect.w;
    let mut bottom = rect.y + rect.h;
    if world
        .get::<crate::ui::Style>(entity)
        .is_some_and(|style| style.clip_children)
    {
        return Some((right, bottom));
    }
    if let Some(children) = world.get::<crate::ui::Children>(entity) {
        for &child in &children.0 {
            if let Some((child_right, child_bottom)) = descendant_extent(world, child) {
                right = right.max(child_right);
                bottom = bottom.max(child_bottom);
            }
        }
    }
    Some((right, bottom))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScrollBounds {
    pub viewport_width: Fixed,
    pub viewport_height: Fixed,
    pub content_width: Fixed,
    pub content_height: Fixed,
    pub max_x: Fixed,
    pub max_y: Fixed,
}

pub fn scroll_bounds(world: &World, entity: Entity) -> Option<ScrollBounds> {
    let rect = world.get::<crate::ui::ComputedRect>(entity)?.0;
    let mut content_width = rect.w;
    let mut content_height = rect.h;
    if let Some(children) = world.get::<crate::ui::Children>(entity) {
        for &child in &children.0 {
            if let Some((right, bottom)) = descendant_extent(world, child) {
                content_width = content_width.max(right - rect.x);
                content_height = content_height.max(bottom - rect.y);
            }
        }
    }
    if let Some(config) = world.get::<ScrollConfig>(entity) {
        if config.content_width > Fixed::ZERO {
            content_width = config.content_width;
        }
        if config.content_height > Fixed::ZERO {
            content_height = config.content_height;
        }
    }
    if let Some((horizontal, extent)) = crate::ui::widgets::lazy_list::content_extent(world, entity)
    {
        if horizontal {
            content_width = extent;
            content_height = rect.h;
        } else {
            content_height = extent;
            content_width = rect.w;
        }
    }
    Some(ScrollBounds {
        viewport_width: rect.w,
        viewport_height: rect.h,
        content_width,
        content_height,
        max_x: (content_width - rect.w).max(Fixed::ZERO),
        max_y: (content_height - rect.h).max(Fixed::ZERO),
    })
}

fn accumulate_scroll_delta(world: &mut World, target: Entity, dx: Fixed, dy: Fixed) {
    if dx == Fixed::ZERO && dy == Fixed::ZERO {
        return;
    }
    if let Some(d) = world.get_mut::<ScrollDelta>(target) {
        d.dx += dx;
        d.dy += dy;
    } else {
        world.insert(target, ScrollDelta { dx, dy });
    }
}

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
    pub(crate) velocity: super::velocity::VelocityTracker,
}

#[derive(Default)]
pub struct ScrollSpring {
    pub x: Option<Spring>,
    pub y: Option<Spring>,
    pub target_entity: Option<Entity>,
    wheel: super::wheel::WheelScroll,
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
            velocity: super::velocity::VelocityTracker::default(),
        }
    }
}

const DIRECTION_THRESHOLD: Fixed = Fixed::from_int(5);

fn scroll_time_ms(world: &World) -> u32 {
    world.resource::<crate::ecs::MonoClock>().map_or_else(
        || (crate::core::time::clock_now_ns() / 1_000_000) as u32,
        |clock| clock.now_ms(),
    )
}

pub fn scroll_system(
    world: &mut World,
    root: Entity,
    event: &InputEvent,
    screen_w: u16,
    screen_h: u16,
) {
    let pointer_down_target = match event {
        InputEvent::PointerDown { x, y, .. } => hit_test(world, root, *x, *y, screen_w, screen_h),
        _ => None,
    };
    let now_ms = scroll_time_ms(world);
    scroll_system_with_target(
        world,
        root,
        event,
        screen_w,
        screen_h,
        pointer_down_target,
        now_ms,
    );
}

pub(crate) fn cancel_scroll_drag(world: &mut World) {
    let target = world
        .resource::<ScrollDragState>()
        .filter(|state| state.active && state.resolved)
        .map(|state| state.target);
    if let Some(target) = target {
        let bounds = scroll_bounds(world, target).unwrap_or_default();
        let mut applied_dx = Fixed::ZERO;
        let mut applied_dy = Fixed::ZERO;
        if let Some(offset) = world.get_mut::<ScrollOffset>(target) {
            let old_x = offset.x;
            let old_y = offset.y;
            offset.x = offset.x.clamp(Fixed::ZERO, bounds.max_x);
            offset.y = offset.y.clamp(Fixed::ZERO, bounds.max_y);
            applied_dx = offset.x - old_x;
            applied_dy = offset.y - old_y;
        }
        accumulate_scroll_delta(world, target, applied_dx, applied_dy);
        if applied_dx != Fixed::ZERO || applied_dy != Fixed::ZERO {
            world.invalidate_visual(target);
        }
    }
    if let Some(spring) = world.resource_mut::<ScrollSpring>() {
        spring.x = None;
        spring.y = None;
        spring.target_entity = None;
        spring.wheel = super::wheel::WheelScroll::default();
    }
    if let Some(state) = world.resource_mut::<ScrollDragState>() {
        state.active = false;
        state.resolved = false;
        state.vel_x = Fixed::ZERO;
        state.vel_y = Fixed::ZERO;
    }
}

/// Reports Wheel/Rotary acceptance; pointer ownership remains in ScrollDragState.
#[allow(clippy::too_many_arguments)]
pub(crate) fn scroll_system_with_target(
    world: &mut World,
    root: Entity,
    event: &InputEvent,
    screen_w: u16,
    screen_h: u16,
    pointer_down_target: Option<Entity>,
    now_ms: u32,
) -> bool {
    let mut accepted = false;
    match event {
        InputEvent::PointerDown { x, y, .. } => {
            if let Some(state) = world.resource_mut::<ScrollDragState>() {
                state.active = true;
                state.resolved = false;
                state.hit_entity = pointer_down_target.unwrap_or(Entity {
                    id: 0,
                    generation: 0,
                });
                state.start_x = *x;
                state.start_y = *y;
                state.last_x = *x;
                state.last_y = *y;
                state.vel_x = Fixed::ZERO;
                state.vel_y = Fixed::ZERO;
                state.velocity = super::velocity::VelocityTracker::default();
                state.velocity.record(*x, *y, now_ms);
            }
            if let Some(ss) = world.resource_mut::<ScrollSpring>() {
                ss.x = None;
                ss.y = None;
                ss.target_entity = None;
                ss.wheel = super::wheel::WheelScroll::default();
            }
        }
        InputEvent::PointerMove { x, y, .. } => {
            if let Some(state) = world.resource_mut::<ScrollDragState>()
                && state.active
            {
                state.velocity.record(*x, *y, now_ms);
            }
            let (active, resolved, _target, hit_entity, last_x, last_y, start_x, start_y) = {
                let Some(state) = world.resource::<ScrollDragState>() else {
                    return false;
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
                return false;
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
                    return false;
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
                let found = pointer_scroll_allowed(world, hit_entity, gesture_dir)
                    .then(|| {
                        find_scroll_target_for_direction(
                            world,
                            hit_entity,
                            gesture_dir,
                            scroll_dx,
                            scroll_dy,
                        )
                    })
                    .flatten();

                if let Some(state) = world.resource_mut::<ScrollDragState>() {
                    state.resolved = true;
                    if let Some(t) = found {
                        state.target = t;
                        state.last_resolved_target = Some(t);
                    } else {
                        // No matching scroll target — deactivate
                        state.active = false;
                        return false;
                    }
                }
            }

            // Apply scroll delta — no mid-drag chaining (iOS behavior)
            // Chain decision was made at resolve time. During drag, always apply to target.
            let (target,) = {
                let Some(state) = world.resource::<ScrollDragState>() else {
                    return false;
                };
                (state.target,)
            };

            let dx = *x - last_x;
            let dy = *y - last_y;

            let config = world.get::<ScrollConfig>(target);
            let dir = config.map(|c| c.direction).unwrap_or(ScrollAxis::Vertical);
            let elastic = config.map(|c| c.elastic).unwrap_or(true);
            let bounds = scroll_bounds(world, target).unwrap_or_default();
            let (max_x, max_y) = (bounds.max_x, bounds.max_y);

            // elastic=true rubber-bands past the edges; elastic=false hard-clamps.
            let bound = |offset: Fixed, delta: Fixed, max: Fixed, viewport: Fixed| -> Fixed {
                if elastic {
                    elastic_resist(offset, delta, max, viewport)
                } else {
                    let target = (offset + delta).clamp(Fixed::ZERO, max);
                    target - offset
                }
            };

            let mut applied_dx = Fixed::ZERO;
            let mut applied_dy = Fixed::ZERO;
            if let Some(scroll) = world.get_mut::<ScrollOffset>(target) {
                match dir {
                    ScrollAxis::Vertical => {
                        let d = bound(scroll.y, -dy, max_y, bounds.viewport_height);
                        scroll.y += d;
                        applied_dy = d;
                    }
                    ScrollAxis::Horizontal => {
                        let d = bound(scroll.x, -dx, max_x, bounds.viewport_width);
                        scroll.x += d;
                        applied_dx = d;
                    }
                    ScrollAxis::Both => {
                        let dxa = bound(scroll.x, -dx, max_x, bounds.viewport_width);
                        let dya = bound(scroll.y, -dy, max_y, bounds.viewport_height);
                        scroll.x += dxa;
                        scroll.y += dya;
                        applied_dx = dxa;
                        applied_dy = dya;
                    }
                }
            }
            accumulate_scroll_delta(world, target, applied_dx, applied_dy);
            // Stuck-at-edge drags translate to zero applied delta; no
            // need to redraw a frame whose pixels didn't move.
            if applied_dx != Fixed::ZERO || applied_dy != Fixed::ZERO {
                world.invalidate_visual(target);
            }

            let dir = world
                .get::<ScrollConfig>(target)
                .map(|c| c.direction)
                .unwrap_or(ScrollAxis::Vertical);
            if let Some(state) = world.resource_mut::<ScrollDragState>() {
                let (velocity_x, velocity_y) = state.velocity.velocity(now_ms);
                match dir {
                    ScrollAxis::Vertical => {
                        state.vel_x = Fixed::ZERO;
                        state.vel_y = velocity_y;
                    }
                    ScrollAxis::Horizontal => {
                        state.vel_x = velocity_x;
                        state.vel_y = Fixed::ZERO;
                    }
                    ScrollAxis::Both => {
                        state.vel_x = velocity_x;
                        state.vel_y = velocity_y;
                    }
                }
                state.last_x = *x;
                state.last_y = *y;
            }
        }
        InputEvent::PointerCancel { .. } | InputEvent::AppSuspend => {
            cancel_scroll_drag(world);
        }
        InputEvent::PointerUp { .. } => {
            let (vel_x, vel_y, target_entity, dir, elastic) = {
                let Some(state) = world.resource::<ScrollDragState>() else {
                    return false;
                };
                if !state.resolved {
                    if let Some(state) = world.resource_mut::<ScrollDragState>() {
                        state.active = false;
                    }
                    return false;
                }
                let t = state.target;
                let cfg = world.get::<ScrollConfig>(t);
                let dir = cfg.map(|c| c.direction).unwrap_or(ScrollAxis::Vertical);
                let elastic = cfg.map(|c| c.elastic).unwrap_or(true);
                let (vel_x, vel_y) = state.velocity.velocity(now_ms);
                (vel_x, vel_y, t, dir, elastic)
            };

            let (offset_x, offset_y) = world
                .get::<ScrollOffset>(target_entity)
                .map(|s| (s.x, s.y))
                .unwrap_or((Fixed::ZERO, Fixed::ZERO));

            let bounds = scroll_bounds(world, target_entity).unwrap_or_default();
            let (max_x, max_y) = (bounds.max_x, bounds.max_y);

            let release = |offset: Fixed, velocity: Fixed, max: Fixed| {
                if offset < Fixed::ZERO || offset > max {
                    return Some(Spring::preset(
                        offset,
                        offset.clamp(Fixed::ZERO, max),
                        if elastic { BOUNCY } else { SMOOTH },
                    ));
                }
                if velocity == Fixed::ZERO {
                    return None;
                }
                let projected =
                    (offset + velocity * Fixed::from_ratio(120, 1000)).clamp(Fixed::ZERO, max);
                Some(Spring::preset(offset, projected, SMOOTH).with_velocity(velocity))
            };

            let spring_x = match dir {
                ScrollAxis::Horizontal | ScrollAxis::Both => release(offset_x, vel_x, max_x),
                _ => None,
            };
            let spring_y = match dir {
                ScrollAxis::Vertical | ScrollAxis::Both => release(offset_y, vel_y, max_y),
                _ => None,
            };

            if let Some(ss) = world.resource_mut::<ScrollSpring>() {
                ss.x = spring_x;
                ss.y = spring_y;
                ss.target_entity = Some(target_entity);
                ss.wheel = super::wheel::WheelScroll::default();
            }

            if let Some(state) = world.resource_mut::<ScrollDragState>() {
                state.active = false;
                state.resolved = false;
            }
        }
        InputEvent::Rotary { delta, .. } => {
            let target = world
                .resource::<ScrollDragState>()
                .and_then(|s| s.last_resolved_target);
            if let Some(target) = target {
                if !world.is_alive(target) {
                    return false;
                }
                let step = Fixed::from_int(20);
                let offset = Fixed::from(*delta as i32) * step;
                let axis = world
                    .get::<ScrollConfig>(target)
                    .map(|c| c.direction)
                    .unwrap_or(ScrollAxis::Vertical);
                let mut applied_dx = Fixed::ZERO;
                let mut applied_dy = Fixed::ZERO;
                if let Some(scroll) = world.get_mut::<ScrollOffset>(target) {
                    accepted = true;
                    match axis {
                        ScrollAxis::Vertical => {
                            scroll.y -= offset;
                            applied_dy = -offset;
                        }
                        ScrollAxis::Horizontal => {
                            scroll.x -= offset;
                            applied_dx = -offset;
                        }
                        ScrollAxis::Both => {
                            scroll.y -= offset;
                            applied_dy = -offset;
                        }
                    }
                }
                if accepted {
                    accumulate_scroll_delta(world, target, applied_dx, applied_dy);
                    world.invalidate_visual(target);
                }
            }
        }
        InputEvent::Wheel { dx, dy, x, y } => {
            let Some(hit) = hit_test(world, root, *x, *y, screen_w, screen_h) else {
                return false;
            };
            let delta_x = *dx;
            let delta_y = -*dy;
            let gesture_dir = if dy.abs() >= dx.abs() {
                ScrollAxis::Vertical
            } else {
                ScrollAxis::Horizontal
            };
            let Some(target) =
                find_scroll_target_for_direction(world, hit, gesture_dir, delta_x, delta_y)
            else {
                return false;
            };

            let (axis, bounds, elastic) = {
                let cfg = world.get::<ScrollConfig>(target);
                let bounds = scroll_bounds(world, target).unwrap_or_default();
                let dir = cfg.map(|c| c.direction).unwrap_or(ScrollAxis::Vertical);
                let elastic = cfg.map(|c| c.elastic).unwrap_or(true);
                (dir, bounds, elastic)
            };
            let (max_x, max_y) = (bounds.max_x, bounds.max_y);

            let bound = |offset: Fixed, delta: Fixed, max: Fixed, viewport: Fixed| {
                if elastic {
                    offset + elastic_resist(offset, delta, max, viewport)
                } else {
                    (offset + delta).clamp(Fixed::ZERO, max)
                }
            };
            let (offset_x, offset_y) = world
                .get::<ScrollOffset>(target)
                .map(|s| (s.x, s.y))
                .unwrap_or((Fixed::ZERO, Fixed::ZERO));
            let needs_x =
                matches!(axis, ScrollAxis::Horizontal | ScrollAxis::Both) && delta_x != Fixed::ZERO;
            let needs_y =
                matches!(axis, ScrollAxis::Vertical | ScrollAxis::Both) && delta_y != Fixed::ZERO;
            let direct = || {
                (
                    if needs_x {
                        bound(offset_x, delta_x, max_x, bounds.viewport_width)
                    } else {
                        offset_x
                    },
                    if needs_y {
                        bound(offset_y, delta_y, max_y, bounds.viewport_height)
                    } else {
                        offset_y
                    },
                )
            };
            accepted = needs_x || needs_y;
            let (next_x, next_y) =
                if accepted && let Some(ss) = world.resource_mut::<ScrollSpring>() {
                    ss.x = None;
                    ss.y = None;
                    ss.target_entity = None;
                    if elastic {
                        ss.wheel.input(
                            target,
                            (offset_x, offset_y),
                            (
                                if needs_x { delta_x } else { Fixed::ZERO },
                                if needs_y { delta_y } else { Fixed::ZERO },
                            ),
                            bounds,
                            now_ms,
                        )
                    } else {
                        ss.wheel = super::wheel::WheelScroll::default();
                        direct()
                    }
                } else {
                    direct()
                };
            if let Some(offset) = world.get_mut::<ScrollOffset>(target) {
                offset.x = next_x;
                offset.y = next_y;
            }
            if next_x != offset_x || next_y != offset_y {
                accumulate_scroll_delta(world, target, next_x - offset_x, next_y - offset_y);
                world.invalidate_visual(target);
            }
        }
        _ => {}
    }
    accepted
}

/// Inertia system — call every frame to decelerate scroll after release
pub fn scroll_inertia_system(world: &mut World) -> Result<(), crate::ecs::CommandError> {
    let active = {
        let Some(state) = world.resource::<ScrollDragState>() else {
            return Ok(());
        };
        state.active
    };

    if active {
        return Ok(());
    }

    let (has_spring, target_entity, pulling) = {
        let Some(ss) = world.resource::<ScrollSpring>() else {
            return Ok(());
        };
        (
            ss.x.is_some() || ss.y.is_some() || ss.wheel.active_target().is_some(),
            ss.wheel.active_target().or(ss.target_entity),
            ss.wheel.is_pulling(),
        )
    };

    if !has_spring {
        return Ok(());
    }

    let Some(target) = target_entity else {
        return Ok(());
    };

    let dt = world
        .resource::<crate::ecs::DeltaTimeMs>()
        .map_or(16, |r| r.0);

    let (bounds, elastic) = {
        let config = world.get::<ScrollConfig>(target);
        let bounds = scroll_bounds(world, target).unwrap_or_default();
        let elastic = config.map(|c| c.elastic).unwrap_or(true);
        (bounds, elastic)
    };
    let (max_x, max_y) = (bounds.max_x, bounds.max_y);

    let now_ms = if pulling { scroll_time_ms(world) } else { 0 };
    let offset = world.get::<ScrollOffset>(target).copied();
    let (new_x, new_y, done) = {
        let Some(ss) = world.resource_mut::<ScrollSpring>() else {
            return Ok(());
        };

        if ss.wheel.active_target().is_some() {
            let (mut x, mut y) = ss.wheel.tick(dt, bounds, now_ms);
            if !elastic {
                x = offset.map(|value| value.x.clamp(Fixed::ZERO, max_x));
                y = offset.map(|value| value.y.clamp(Fixed::ZERO, max_y));
                ss.wheel = super::wheel::WheelScroll::default();
            }
            (x, y, false)
        } else {
            let mut done = true;

            // elastic=true bounces past the edge with a softer curve;
            // elastic=false snaps to the edge with the same smooth preset
            // the spring already uses, never letting the offset leave bounds.
            let edge_curve = if elastic { Some(BOUNCY) } else { Some(SMOOTH) };

            if let Some(ref mut sx) = ss.x {
                tick_scroll_spring(sx, dt, max_x, bounds.viewport_width, elastic);
                let pos = sx.value();
                if pos < Fixed::ZERO {
                    sx.retarget(Fixed::ZERO, edge_curve);
                } else if pos > max_x {
                    sx.retarget(max_x, edge_curve);
                }
                if !sx.is_settled() {
                    done = false;
                }
            }

            if let Some(ref mut sy) = ss.y {
                tick_scroll_spring(sy, dt, max_y, bounds.viewport_height, elastic);
                let pos = sy.value();
                if pos < Fixed::ZERO {
                    sy.retarget(Fixed::ZERO, edge_curve);
                } else if pos > max_y {
                    sy.retarget(max_y, edge_curve);
                }
                if !sy.is_settled() {
                    done = false;
                }
            }

            let nx = ss.x.as_ref().map(|s| {
                if elastic {
                    s.value()
                } else {
                    s.value().clamp(Fixed::ZERO, max_x)
                }
            });
            let ny = ss.y.as_ref().map(|s| {
                if elastic {
                    s.value()
                } else {
                    s.value().clamp(Fixed::ZERO, max_y)
                }
            });
            (nx, ny, done)
        }
    };

    let mut changed = false;
    let mut applied_dx = Fixed::ZERO;
    let mut applied_dy = Fixed::ZERO;
    if let Some(scroll) = world.get_mut::<ScrollOffset>(target) {
        if let Some(nx) = new_x {
            // >=1px gate would skip spring-tail sub-pixel drift, leaving
            // ScrollOffset and screen out of sync at settle.
            if nx != scroll.x {
                changed = true;
                applied_dx = nx - scroll.x;
            }
            scroll.x = nx;
        }
        if let Some(ny) = new_y {
            if ny != scroll.y {
                changed = true;
                applied_dy = ny - scroll.y;
            }
            scroll.y = ny;
        }
    }
    if changed {
        accumulate_scroll_delta(world, target, applied_dx, applied_dy);
        world.invalidate_visual(target);
    }

    if done {
        if let Some(ss) = world.resource_mut::<ScrollSpring>() {
            ss.x = None;
            ss.y = None;
            ss.target_entity = None;
        }
    }
    Ok(())
}

crate::ecs::system::framework_system!(pub scroll_inertia_system, ScrollInertia);

fn pointer_scroll_allowed(world: &World, start: Entity, gesture_dir: ScrollAxis) -> bool {
    let mut current = start;
    loop {
        if let Some(action) = world.get::<TouchAction>(current).copied()
            && action != TouchAction::Auto
        {
            return action.allows_scroll(gesture_dir);
        }
        if let Some(parent) = world.get::<crate::ui::Parent>(current) {
            current = parent.0;
        } else {
            return true;
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
        if let Some(parent) = world.get::<crate::ui::Parent>(current) {
            current = parent.0;
        } else {
            break;
        }
    }
    last_matching
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fallback_scroll_world() -> (World, Entity) {
        use crate::types::{Dimension, Viewport};
        use crate::ui::layout::LayoutStyle;
        use crate::ui::{Style, Widget};

        let mut world = World::new();
        let root = world.spawn(Widget);
        world.insert(
            root,
            Style {
                layout: LayoutStyle {
                    width: Dimension::px(100),
                    height: Dimension::px(100),
                    ..LayoutStyle::default()
                },
                ..Style::default()
            },
        );
        world.insert(root, ScrollOffset::default());
        world.insert(
            root,
            ScrollConfig {
                content_height: Fixed::from_int(300),
                elastic: false,
                ..ScrollConfig::default()
            },
        );
        world.insert(root, ScrollDelta::default());
        world.insert_resource(ScrollDragState {
            last_resolved_target: Some(root),
            ..ScrollDragState::default()
        });
        world.insert_resource(ScrollSpring::default());
        crate::ui::render_system::update_layout(
            &mut world,
            root,
            &Viewport::new(100, 100, Fixed::ONE),
        );
        world.invalidate_visual(root);
        (world, root)
    }

    #[test]
    fn wheel_acceptance_tracks_scroll_handling_not_offset_changes() {
        let (mut world, root) = fallback_scroll_world();
        let event = InputEvent::Wheel {
            dx: Fixed::ZERO,
            dy: Fixed::from_int(-20),
            x: 20.into(),
            y: 20.into(),
        };
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                assert!(scroll_system_with_target(
                    &mut world, root, &event, 100, 100, None, 0
                ));
            }),
            0
        );
        assert_eq!(
            world.get::<ScrollOffset>(root).unwrap().y,
            Fixed::from_int(20)
        );
        assert!(world.resource::<ScrollSpring>().unwrap().y.is_none());

        world.get_mut::<ScrollOffset>(root).unwrap().y = Fixed::from_int(200);
        world.insert_resource(ScrollSpring::default());
        assert!(scroll_system_with_target(
            &mut world, root, &event, 100, 100, None, 0
        ));
        assert_eq!(
            world.get::<ScrollOffset>(root).unwrap().y,
            Fixed::from_int(200)
        );
        assert!(world.resource::<ScrollSpring>().unwrap().y.is_none());
    }

    #[test]
    fn wheel_requires_a_matching_target_but_not_inertia_storage() {
        let (mut world, root) = fallback_scroll_world();
        for event in [
            InputEvent::Wheel {
                dx: Fixed::ONE,
                dy: Fixed::ZERO,
                x: 20.into(),
                y: 20.into(),
            },
            InputEvent::Wheel {
                dx: Fixed::ZERO,
                dy: Fixed::ONE,
                x: 120.into(),
                y: 20.into(),
            },
        ] {
            assert!(!scroll_system_with_target(
                &mut world, root, &event, 100, 100, None, 0
            ));
        }
        assert!(
            world
                .resource::<ScrollSpring>()
                .unwrap()
                .target_entity
                .is_none()
        );
        world.remove_resource::<ScrollSpring>();
        let event = InputEvent::Wheel {
            dx: Fixed::ZERO,
            dy: Fixed::from_int(-20),
            x: 20.into(),
            y: 20.into(),
        };
        assert!(scroll_system_with_target(
            &mut world, root, &event, 100, 100, None, 0
        ));
        assert_eq!(
            world.get::<ScrollOffset>(root).unwrap().y,
            Fixed::from_int(20)
        );
    }

    #[test]
    fn drag_release_after_stopping_has_no_snap_but_a_flick_has_inertia() {
        for pause in [0, 100] {
            let (mut world, root) = fallback_scroll_world();
            world.get_mut::<ScrollOffset>(root).unwrap().y = 50.into();
            scroll_system_with_target(
                &mut world,
                root,
                &InputEvent::PointerDown {
                    id: 0,
                    x: 20.into(),
                    y: 70.into(),
                },
                100,
                100,
                Some(root),
                0,
            );
            for step in 1..=3 {
                scroll_system_with_target(
                    &mut world,
                    root,
                    &InputEvent::PointerMove {
                        id: 0,
                        x: 20.into(),
                        y: (70 - step * 10).into(),
                    },
                    100,
                    100,
                    None,
                    (step * 10) as u32,
                );
            }
            let offset = world.get::<ScrollOffset>(root).unwrap().y;
            scroll_system_with_target(
                &mut world,
                root,
                &InputEvent::PointerUp {
                    id: 0,
                    x: 20.into(),
                    y: 40.into(),
                },
                100,
                100,
                None,
                30 + pause,
            );
            assert_eq!(
                world.resource::<ScrollSpring>().unwrap().y.is_some(),
                pause == 0
            );
            world.insert_resource(crate::ecs::DeltaTimeMs(16));
            scroll_inertia_system(&mut world).unwrap();
            let next = world.get::<ScrollOffset>(root).unwrap().y;
            if pause == 0 {
                assert!(next > offset);
            } else {
                assert_eq!(next, offset);
            }
        }
    }

    #[test]
    fn fractional_wheel_moves_immediately_without_a_second_inertia_curve() {
        let (mut world, root) = fallback_scroll_world();
        world.get_mut::<ScrollConfig>(root).unwrap().direction = ScrollAxis::Horizontal;
        world.get_mut::<ScrollConfig>(root).unwrap().content_width = 300.into();
        let event = InputEvent::Wheel {
            dx: Fixed::from_ratio(1, 4),
            dy: Fixed::from_ratio(1, 8),
            x: 20.into(),
            y: 20.into(),
        };
        for i in 1..=4 {
            assert!(scroll_system_with_target(
                &mut world, root, &event, 100, 100, None, i
            ));
            assert_eq!(
                world.get::<ScrollOffset>(root).unwrap().x,
                Fixed::from_ratio(i as i32, 4)
            );
        }
        world.insert_resource(crate::ecs::DeltaTimeMs(16));
        for _ in 0..30 {
            scroll_inertia_system(&mut world).unwrap();
        }
        assert_eq!(world.get::<ScrollOffset>(root).unwrap().x, Fixed::ONE);
    }

    #[test]
    fn rubber_band_preserves_in_bounds_distance_and_event_batching() {
        let max = Fixed::from_int(200);
        let viewport = Fixed::from_int(240);
        assert_eq!(
            elastic_resist(50.into(), 30.into(), max, viewport),
            30.into()
        );
        assert_eq!(
            elastic_resist((-20).into(), 5.into(), max, viewport),
            5.into()
        );
        let single = elastic_resist(Fixed::ZERO, (-120).into(), max, viewport);
        for step in [1, 4, 30] {
            let mut top = Fixed::ZERO;
            let mut bottom = max;
            for _ in 0..120 / step {
                top += elastic_resist(top, Fixed::from_int(-step), max, viewport);
                bottom += elastic_resist(bottom, Fixed::from_int(step), max, viewport);
            }
            assert!((top - single).abs() < Fixed::ONE);
            assert_eq!(top, max - bottom);
        }
    }

    #[test]
    fn wheel_tail_is_owned_by_rebound_until_settled_without_allocating() {
        for interval in [8, 16, 32] {
            let (mut world, root) = fallback_scroll_world();
            world.get_mut::<ScrollConfig>(root).unwrap().elastic = true;
            world.get_mut::<ScrollOffset>(root).unwrap().y = Fixed::from_int(3 * interval);
            world.insert_resource(crate::ecs::DeltaTimeMs(interval as u16));
            for frame in 0..4 {
                scroll_system_with_target(
                    &mut world,
                    root,
                    &InputEvent::Wheel {
                        dx: Fixed::ZERO,
                        dy: Fixed::from_int(interval),
                        x: 20.into(),
                        y: 20.into(),
                    },
                    100,
                    100,
                    None,
                    (frame * interval) as u32,
                );
            }
            assert!(
                world
                    .resource::<ScrollSpring>()
                    .unwrap()
                    .wheel
                    .active_target()
                    .is_some()
            );
            let initial = world.get::<ScrollOffset>(root).unwrap().y;
            assert!(initial < Fixed::ZERO);
            assert_eq!(
                crate::test_support::tracked_allocations(|| {
                    for frame in 4..400 {
                        let before = world.get::<ScrollOffset>(root).unwrap().y;
                        let event = InputEvent::Wheel {
                            dx: Fixed::ZERO,
                            dy: Fixed::from_ratio(1, 4),
                            x: 20.into(),
                            y: 20.into(),
                        };
                        assert!(scroll_system_with_target(
                            &mut world,
                            root,
                            &event,
                            100,
                            100,
                            None,
                            (frame * interval) as u32
                        ));
                        assert_eq!(world.get::<ScrollOffset>(root).unwrap().y, before);
                        scroll_inertia_system(&mut world).unwrap();
                        let position = world.get::<ScrollOffset>(root).unwrap().y;
                        assert!(position <= Fixed::ZERO);
                        if frame == 4 {
                            assert!(position < initial);
                        }
                    }
                }),
                0
            );
            assert_eq!(world.get::<ScrollOffset>(root).unwrap().y, Fixed::ZERO);
            assert!(
                world
                    .resource::<ScrollSpring>()
                    .unwrap()
                    .wheel
                    .active_target()
                    .is_none()
            );
        }
    }

    #[test]
    fn stationary_wheel_pull_releases_and_can_be_disabled_without_allocating() {
        let _clock = crate::core::time::mock::install();
        crate::core::time::mock::set_ms(0);
        let (mut world, root) = fallback_scroll_world();
        world.get_mut::<ScrollConfig>(root).unwrap().elastic = true;
        world.insert_resource(crate::ecs::DeltaTimeMs(8));
        let event = InputEvent::Wheel {
            dx: Fixed::ZERO,
            dy: Fixed::from_ratio(1, 4),
            x: 20.into(),
            y: 20.into(),
        };
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                for frame in 0..32 {
                    crate::core::time::mock::set_ms(frame * 8);
                    let before = world.get::<ScrollOffset>(root).unwrap().y;
                    scroll_system(&mut world, root, &event, 100, 100);
                    let after = world.get::<ScrollOffset>(root).unwrap().y;
                    assert!(after < before);
                    scroll_inertia_system(&mut world).unwrap();
                    assert_eq!(world.get::<ScrollOffset>(root).unwrap().y, after);
                }
                for frame in 32..400 {
                    crate::core::time::mock::set_ms(frame * 8);
                    let before = world.get::<ScrollOffset>(root).unwrap().y;
                    scroll_inertia_system(&mut world).unwrap();
                    let after = world.get::<ScrollOffset>(root).unwrap().y;
                    assert!(after >= before && after <= Fixed::ZERO);
                }
            }),
            0
        );
        assert_eq!(world.get::<ScrollOffset>(root).unwrap().y, Fixed::ZERO);
        scroll_system(&mut world, root, &event, 100, 100);
        assert!(world.get::<ScrollOffset>(root).unwrap().y < Fixed::ZERO);
        world.get_mut::<ScrollConfig>(root).unwrap().elastic = false;
        scroll_inertia_system(&mut world).unwrap();
        assert_eq!(world.get::<ScrollOffset>(root).unwrap().y, Fixed::ZERO);
    }

    #[test]
    fn rotary_acceptance_uses_live_last_target_and_retained_offset() {
        let (mut world, root) = fallback_scroll_world();
        let other = world.spawn(ScrollOffset::default());
        world.insert(
            other,
            ScrollConfig {
                direction: ScrollAxis::Horizontal,
                ..ScrollConfig::default()
            },
        );
        world.insert(other, ScrollDelta::default());
        world.invalidate_visual(other);
        world
            .resource_mut::<ScrollDragState>()
            .unwrap()
            .last_resolved_target = Some(other);
        let event = InputEvent::Rotary { id: 3, delta: -2 };
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                assert!(scroll_system_with_target(
                    &mut world, root, &event, 100, 100, None, 0
                ));
            }),
            0
        );
        assert_eq!(
            world.get::<ScrollOffset>(other).unwrap().x,
            Fixed::from_int(40)
        );
        assert_eq!(world.get::<ScrollOffset>(root).unwrap().y, Fixed::ZERO);

        world.remove::<ScrollOffset>(other);
        world.remove::<crate::ui::dirty::Dirty>(other);
        assert!(!scroll_system_with_target(
            &mut world, root, &event, 100, 100, None, 0
        ));
        assert!(!world.has::<crate::ui::dirty::Dirty>(other));
        assert!(world.despawn(other));
        let replacement = world.spawn(ScrollOffset::default());
        assert_eq!(replacement.id, other.id);
        assert_ne!(replacement.generation, other.generation);
        assert!(!scroll_system_with_target(
            &mut world, root, &event, 100, 100, None, 0
        ));
        assert_eq!(
            world.get::<ScrollOffset>(replacement).unwrap().x,
            Fixed::ZERO
        );
        assert!(!world.has::<crate::ui::dirty::Dirty>(replacement));
    }

    #[test]
    fn zero_extent_tracks_visible_descendants_without_allocation() {
        use crate::ui::{Children, ComputedRect, Style};

        let mut world = World::new();
        let viewport = world.spawn(Style {
            clip_children: true,
            ..Style::default()
        });
        let content = world.spawn(Style::default());
        let overflow = world.spawn(Style::default());
        let clipped = world.spawn(Style {
            clip_children: true,
            ..Style::default()
        });
        let hidden_overflow = world.spawn(Style::default());
        world.insert(
            viewport,
            ComputedRect(crate::types::Rect::new(10, 20, 120, 80)),
        );
        world.insert(
            content,
            ComputedRect(crate::types::Rect::new(10, 20, 120, 100)),
        );
        world.insert(
            overflow,
            ComputedRect(crate::types::Rect::new(10, 200, 120, 40)),
        );
        world.insert(
            clipped,
            ComputedRect(crate::types::Rect::new(10, 140, 120, 30)),
        );
        world.insert(
            hidden_overflow,
            ComputedRect(crate::types::Rect::new(10, 500, 120, 40)),
        );
        world.insert(viewport, Children(alloc::vec![content]));
        world.insert(content, Children(alloc::vec![overflow, clipped]));
        world.insert(clipped, Children(alloc::vec![hidden_overflow]));
        world.insert(viewport, ScrollConfig::default());

        let bounds = scroll_bounds(&world, viewport).unwrap();

        assert_eq!(bounds.viewport_height, Fixed::from_int(80));
        assert_eq!(bounds.content_height, Fixed::from_int(220));
        assert_eq!(bounds.max_y, Fixed::from_int(140));
    }

    #[test]
    fn explicit_extent_overrides_retained_descendants() {
        use crate::ui::{Children, ComputedRect, Style};

        let mut world = World::new();
        let viewport = world.spawn(Style::default());
        let child = world.spawn(Style::default());
        world.insert(
            viewport,
            ComputedRect(crate::types::Rect::new(0, 0, 100, 80)),
        );
        world.insert(child, ComputedRect(crate::types::Rect::new(0, 0, 100, 200)));
        world.insert(viewport, Children(alloc::vec![child]));
        world.insert(
            viewport,
            ScrollConfig {
                content_height: Fixed::from_int(480),
                ..ScrollConfig::default()
            },
        );

        let bounds = scroll_bounds(&world, viewport).unwrap();

        assert_eq!(bounds.content_height, Fixed::from_int(480));
        assert_eq!(bounds.max_y, Fixed::from_int(400));
    }

    #[test]
    fn pointer_down_reuses_the_pipeline_hit_target() {
        let mut world = World::new();
        world.insert_resource(ScrollDragState::default());
        let root = world.spawn_empty();
        let target = world.spawn_empty();
        let event = InputEvent::PointerDown {
            id: 0,
            x: Fixed::from_int(12),
            y: Fixed::from_int(18),
        };

        assert!(!scroll_system_with_target(
            &mut world,
            root,
            &event,
            64,
            64,
            Some(target),
            0
        ));

        let state = world.resource::<ScrollDragState>().unwrap();
        assert!(state.active);
        assert_eq!(state.hit_entity, target);
    }

    fn run_drag(elastic: bool, init_y: i32, dy_per_frame: i32, frames: usize) -> Fixed {
        use crate::ecs::World;
        use crate::input::event::input::InputEvent;
        use crate::input::event::scroll::components::{ScrollAxis, ScrollConfig, ScrollOffset};
        use crate::ui::{ComputedRect, Widget};
        let mut world = World::new();
        world.insert_resource(ScrollDragState::default());
        let target = world.spawn_empty();
        world.insert(target, Widget);
        world.insert(
            target,
            ComputedRect(crate::types::Rect::new(0, 0, 128, 114)),
        );
        world.insert(
            target,
            ScrollOffset {
                x: Fixed::ZERO,
                y: Fixed::from_int(init_y),
            },
        );
        world.insert(
            target,
            ScrollConfig {
                direction: ScrollAxis::Vertical,
                elastic,
                content_height: Fixed::from_int(600),
                content_width: Fixed::ZERO,
            },
        );
        // Simulate PointerDown at (64, 50) hitting target.
        if let Some(state) = world.resource_mut::<ScrollDragState>() {
            state.active = true;
            state.resolved = true;
            state.target = target;
            state.last_resolved_target = Some(target);
            state.last_x = Fixed::from_int(64);
            state.last_y = Fixed::from_int(50);
            state.start_x = Fixed::from_int(64);
            state.start_y = Fixed::from_int(50);
        }
        let mut y = 50;
        for _ in 0..frames {
            y += dy_per_frame;
            let event = InputEvent::PointerMove {
                id: 0,
                x: Fixed::from_int(64),
                y: Fixed::from_int(y),
            };
            scroll_system(&mut world, target, &event, 128, 128);
        }
        world
            .get::<ScrollOffset>(target)
            .map(|s| s.y)
            .unwrap_or(Fixed::ZERO)
    }

    #[test]
    fn elastic_false_at_top_drag_down_does_not_go_negative() {
        // init at top (y=0), drag finger down +5 px each of 10 frames.
        // Expected: scroll.y stays at 0, never goes negative.
        let final_y = run_drag(false, 0, 5, 10);
        assert!(
            final_y >= Fixed::ZERO,
            "scroll.y went negative: {:?}",
            final_y
        );
    }

    #[test]
    fn elastic_false_pointer_up_clamps_inertia_projection() {
        use crate::ecs::World;
        use crate::input::event::input::InputEvent;
        use crate::input::event::scroll::components::{ScrollAxis, ScrollConfig, ScrollOffset};
        use crate::ui::{ComputedRect, Widget};
        let mut world = World::new();
        world.insert_resource(ScrollDragState::default());
        world.insert_resource(ScrollSpring::default());
        let target = world.spawn_empty();
        world.insert(target, Widget);
        world.insert(
            target,
            ComputedRect(crate::types::Rect::new(0, 0, 128, 114)),
        );
        world.insert(
            target,
            ScrollOffset {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
            },
        );
        world.insert(
            target,
            ScrollConfig {
                direction: ScrollAxis::Vertical,
                elastic: false,
                content_height: Fixed::from_int(600),
                content_width: Fixed::ZERO,
            },
        );
        // PointerUp at top with downward velocity (vel_y < 0 = pulling
        // content down past the top edge).
        if let Some(state) = world.resource_mut::<ScrollDragState>() {
            state.active = true;
            state.resolved = true;
            state.target = target;
            state.last_resolved_target = Some(target);
            state.vel_x = Fixed::ZERO;
            state.vel_y = -Fixed::from_int(2);
            state.velocity.record(64.into(), 40.into(), 0);
            state.velocity.record(64.into(), 50.into(), 10);
        }
        let event = InputEvent::PointerUp {
            id: 0,
            x: Fixed::from_int(64),
            y: Fixed::from_int(50),
        };
        scroll_system_with_target(&mut world, target, &event, 128, 128, None, 10);
        assert!(world.resource::<ScrollSpring>().unwrap().y.is_some());
        // Tick the spring forward and assert offset never goes negative.
        world.insert_resource(crate::ecs::DeltaTimeMs(16));
        for _ in 0..30 {
            scroll_inertia_system(&mut world).unwrap();
            let off = world
                .get::<ScrollOffset>(target)
                .map(|s| s.y)
                .unwrap_or(Fixed::ZERO);
            assert!(
                off >= Fixed::ZERO,
                "spring drove offset negative: {:?}",
                off
            );
        }
    }

    #[test]
    fn pointer_cancel_stops_drag_without_inertia() {
        use crate::ui::{ComputedRect, Widget};

        let mut world = World::new();
        world.insert_resource(ScrollDragState::default());
        world.insert_resource(ScrollSpring::default());
        let target = world.spawn_empty();
        world.insert(target, Widget);
        world.insert(
            target,
            ComputedRect(crate::types::Rect::new(0, 0, 128, 100)),
        );
        world.insert(
            target,
            ScrollOffset {
                x: Fixed::ZERO,
                y: Fixed::from_int(40),
            },
        );
        world.insert(
            target,
            ScrollConfig {
                direction: ScrollAxis::Vertical,
                elastic: true,
                content_height: Fixed::from_int(500),
                content_width: Fixed::ZERO,
            },
        );
        if let Some(state) = world.resource_mut::<ScrollDragState>() {
            state.active = true;
            state.resolved = true;
            state.target = target;
            state.vel_y = Fixed::from_int(8);
        }

        scroll_system(
            &mut world,
            target,
            &InputEvent::PointerCancel {
                id: 0,
                x: Fixed::from_int(64),
                y: Fixed::from_int(50),
            },
            128,
            100,
        );

        let state = world.resource::<ScrollDragState>().unwrap();
        assert!(!state.active);
        assert!(!state.resolved);
        assert_eq!(state.vel_x, Fixed::ZERO);
        assert_eq!(state.vel_y, Fixed::ZERO);
        let spring = world.resource::<ScrollSpring>().unwrap();
        assert!(spring.x.is_none());
        assert!(spring.y.is_none());
        assert!(spring.target_entity.is_none());
        assert_eq!(
            world.get::<ScrollOffset>(target).unwrap().y,
            Fixed::from_int(40)
        );
    }

    #[test]
    fn elastic_false_inertia_never_paints_beyond_the_bottom_edge() {
        use crate::ecs::World;
        use crate::input::event::input::InputEvent;
        use crate::input::event::scroll::components::{ScrollAxis, ScrollConfig, ScrollOffset};
        use crate::ui::{ComputedRect, Widget};

        let mut world = World::new();
        world.insert_resource(ScrollDragState::default());
        world.insert_resource(ScrollSpring::default());
        let target = world.spawn_empty();
        world.insert(target, Widget);
        world.insert(
            target,
            ComputedRect(crate::types::Rect::new(0, 0, 128, 100)),
        );
        world.insert(
            target,
            ScrollOffset {
                x: Fixed::ZERO,
                y: Fixed::from_int(380),
            },
        );
        world.insert(
            target,
            ScrollConfig {
                direction: ScrollAxis::Vertical,
                elastic: false,
                content_height: Fixed::from_int(500),
                content_width: Fixed::ZERO,
            },
        );
        if let Some(state) = world.resource_mut::<ScrollDragState>() {
            state.active = true;
            state.resolved = true;
            state.target = target;
            state.last_resolved_target = Some(target);
            state.vel_x = Fixed::ZERO;
            state.vel_y = Fixed::from_int(8);
            state.velocity.record(64.into(), 60.into(), 0);
            state.velocity.record(64.into(), 50.into(), 10);
        }
        scroll_system_with_target(
            &mut world,
            target,
            &InputEvent::PointerUp {
                id: 0,
                x: Fixed::from_int(64),
                y: Fixed::from_int(50),
            },
            128,
            100,
            None,
            10,
        );
        assert!(world.resource::<ScrollSpring>().unwrap().y.is_some());
        world.insert_resource(crate::ecs::DeltaTimeMs(16));

        for _ in 0..30 {
            scroll_inertia_system(&mut world).unwrap();
            let offset = world.get::<ScrollOffset>(target).unwrap().y;
            assert!(
                offset <= Fixed::from_int(400),
                "spring drove offset beyond the bottom edge: {offset:?}",
            );
        }
    }

    #[test]
    fn find_scroll_target_walks_to_matching_axis() {
        use crate::ecs::World;
        use crate::input::event::scroll::components::{ScrollAxis, ScrollConfig, ScrollOffset};
        use crate::ui::{ComputedRect, Widget};
        let mut world = World::new();
        let target = world.spawn_empty();
        world.insert(target, Widget);
        world.insert(
            target,
            ComputedRect(crate::types::Rect::new(0, 0, 128, 114)),
        );
        world.insert(
            target,
            ScrollOffset {
                x: Fixed::ZERO,
                y: Fixed::from_int(100),
            },
        );
        world.insert(
            target,
            ScrollConfig {
                direction: ScrollAxis::Vertical,
                elastic: false,
                content_height: Fixed::from_int(600),
                content_width: Fixed::ZERO,
            },
        );

        let found = find_scroll_target_for_direction(
            &world,
            target,
            ScrollAxis::Vertical,
            Fixed::ZERO,
            Fixed::from_int(40),
        );
        assert_eq!(
            found,
            Some(target),
            "wheel scroll target lookup must resolve a vertical scroller from the hit entity",
        );
    }

    #[test]
    fn nearest_touch_action_arbitrates_pointer_drag_axes() {
        use crate::ecs::World;
        use crate::input::event::scroll::TouchAction;
        use crate::ui::Parent;

        let mut world = World::new();
        let root = world.spawn_empty();
        let parent = world.spawn_empty();
        let child = world.spawn_empty();
        world.insert(parent, Parent(root));
        world.insert(child, Parent(parent));
        world.insert(root, TouchAction::PanY);

        assert!(pointer_scroll_allowed(&world, child, ScrollAxis::Vertical));
        assert!(!pointer_scroll_allowed(
            &world,
            child,
            ScrollAxis::Horizontal
        ));

        world.insert(parent, TouchAction::None);
        assert!(!pointer_scroll_allowed(&world, child, ScrollAxis::Vertical));
        assert!(!pointer_scroll_allowed(
            &world,
            child,
            ScrollAxis::Horizontal
        ));

        world.insert(child, TouchAction::PanX);
        assert!(pointer_scroll_allowed(
            &world,
            child,
            ScrollAxis::Horizontal
        ));
        assert!(!pointer_scroll_allowed(&world, child, ScrollAxis::Vertical));
    }

    #[test]
    fn auto_touch_action_preserves_ancestor_scroll_lookup() {
        use crate::ecs::World;
        use crate::input::event::scroll::TouchAction;
        use crate::ui::Parent;

        let mut world = World::new();
        let parent = world.spawn_empty();
        let child = world.spawn_empty();
        world.insert(child, Parent(parent));
        world.insert(child, TouchAction::Auto);

        assert!(pointer_scroll_allowed(&world, child, ScrollAxis::Vertical));
        assert!(pointer_scroll_allowed(
            &world,
            child,
            ScrollAxis::Horizontal
        ));
    }

    #[test]
    fn drag_accumulates_scroll_delta_across_frames() {
        use crate::ecs::World;
        use crate::input::event::input::InputEvent;
        use crate::input::event::scroll::components::{
            ScrollAxis, ScrollConfig, ScrollDelta, ScrollOffset,
        };
        use crate::ui::{ComputedRect, Widget};
        let mut world = World::new();
        world.insert_resource(ScrollDragState::default());
        let target = world.spawn_empty();
        world.insert(target, Widget);
        world.insert(
            target,
            ComputedRect(crate::types::Rect::new(0, 0, 128, 100)),
        );
        world.insert(
            target,
            ScrollOffset {
                x: Fixed::ZERO,
                y: Fixed::from_int(50),
            },
        );
        world.insert(
            target,
            ScrollConfig {
                direction: ScrollAxis::Vertical,
                elastic: false,
                content_height: Fixed::from_int(500),
                content_width: Fixed::ZERO,
            },
        );
        scroll_system(
            &mut world,
            target,
            &InputEvent::PointerDown {
                id: 0,
                x: Fixed::from_int(64),
                y: Fixed::from_int(50),
            },
            128,
            100,
        );
        let mut y = 50;
        for _ in 0..4 {
            y += 5;
            scroll_system(
                &mut world,
                target,
                &InputEvent::PointerMove {
                    id: 0,
                    x: Fixed::from_int(64),
                    y: Fixed::from_int(y),
                },
                128,
                100,
            );
        }
        let delta = world
            .get::<ScrollDelta>(target)
            .copied()
            .unwrap_or_default();
        assert_eq!(delta.dx, Fixed::ZERO);
        assert!(
            delta.dy.to_int().abs() >= 19,
            "expected |dy| ≈ 20, got {:?}",
            delta.dy
        );
    }
}

fn tick_scroll_spring(
    spring: &mut Spring,
    dt: u16,
    max: Fixed,
    viewport: Fixed,
    resist_outward: bool,
) {
    let previous = spring.value();
    spring.tick(dt);
    if resist_outward {
        let requested = spring.value() - previous;
        let applied = elastic_resist(previous, requested, max, viewport);
        if applied != requested {
            spring.position = previous + applied;
            spring.velocity = spring.velocity * (applied / requested);
        }
    }
}

pub(super) fn elastic_resist(offset: Fixed, delta: Fixed, max: Fixed, viewport: Fixed) -> Fixed {
    let next = offset + delta;
    let (edge, distance, pull, sign) = if next < Fixed::ZERO && delta < Fixed::ZERO {
        (
            Fixed::ZERO,
            (-offset).max(Fixed::ZERO),
            (-delta).min(-next),
            -Fixed::ONE,
        )
    } else if next > max && delta > Fixed::ZERO {
        (
            max,
            (offset - max).max(Fixed::ZERO),
            delta.min(next - max),
            Fixed::ONE,
        )
    } else {
        return delta;
    };
    if viewport <= Fixed::ZERO {
        return edge - offset;
    }
    let distance = distance.min(viewport);
    let remaining = Fixed64::from_fixed(viewport - distance);
    let extent = Fixed64::from_fixed(viewport);
    // Integrate the rubber-band curve in displacement space, independent of event batching.
    let pull = Fixed64::from_fixed(pull) * Fixed64::from_ratio(55, 100) * (remaining / extent);
    let stretch = (remaining * (pull / (extent + pull))).to_fixed();
    edge + sign * (distance + stretch) - offset
}
fn is_at_boundary(world: &World, entity: Entity, delta_x: Fixed, delta_y: Fixed) -> bool {
    let Some(scroll) = world.get::<ScrollOffset>(entity) else {
        return false;
    };
    let config = world.get::<ScrollConfig>(entity);
    let bounds = scroll_bounds(world, entity).unwrap_or_default();
    let (max_x, max_y) = (bounds.max_x, bounds.max_y);

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
