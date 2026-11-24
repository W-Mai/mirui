use super::components::{ScrollAxis, ScrollConfig, ScrollOffset, TouchAction};
use super::controller::{Driver, ScrollMotion, displace};
use crate::ecs::{Entity, World};
use crate::input::event::hit_test::hit_test;
use crate::input::event::input::InputEvent;
use crate::types::{Fixed, Fixed64};

use super::bounds::scroll_bounds;

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
    pub last_resolved_target: Option<Entity>,
    pub(crate) started_at_ms: u32,
    pub(crate) pointer: (crate::surface::PointerKind, u8),
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
            last_resolved_target: None,
            started_at_ms: 0,
            pointer: (crate::surface::PointerKind::Unknown, 0),
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
        crate::input::event::runtime::cancel_scroll(world, target);
        settle_offset(world, target);
    }
    settle_previous_motion(world, None);
    if let Some(state) = world.resource_mut::<ScrollDragState>() {
        state.active = false;
        state.resolved = false;
    }
}

pub(crate) fn set_position(world: &mut World, entity: Entity, position: ScrollOffset) -> bool {
    if !world
        .get::<ScrollOffset>(entity)
        .is_some_and(|old| *old != position)
    {
        return false;
    }
    super::commands::cancel(world, entity);
    take_input_motion(world, entity);
    write_position(world, entity, position)
}

pub(super) fn take_input_motion(world: &mut World, entity: Entity) {
    let dragging = world
        .resource::<ScrollDragState>()
        .is_some_and(|state| state.active && state.resolved && state.target == entity);
    let moving = world
        .resource::<ScrollMotion>()
        .is_some_and(|motion| motion.target_entity == Some(entity));
    if dragging || moving {
        crate::input::event::runtime::cancel_scroll(world, entity);
    }
    if dragging && let Some(state) = world.resource_mut::<ScrollDragState>() {
        state.active = false;
        state.resolved = false;
    }
    if moving && let Some(motion) = world.resource_mut::<ScrollMotion>() {
        *motion = ScrollMotion::default();
    }
}

pub(super) fn write_position(world: &mut World, entity: Entity, position: ScrollOffset) -> bool {
    if world
        .get::<ScrollOffset>(entity)
        .is_none_or(|old| *old == position)
    {
        return false;
    }
    *world.get_mut::<ScrollOffset>(entity).unwrap() = position;
    true
}

fn settle_offset(world: &mut World, target: Entity) {
    let bounds = scroll_bounds(world, target).unwrap_or_default();
    let Some(offset) = world.get_mut::<ScrollOffset>(target) else {
        return;
    };
    let old = (offset.x, offset.y);
    offset.x = offset.x.clamp(Fixed::ZERO, bounds.max_x);
    offset.y = offset.y.clamp(Fixed::ZERO, bounds.max_y);
    let delta = (offset.x - old.0, offset.y - old.1);
    if delta != (Fixed::ZERO, Fixed::ZERO) {
        world.invalidate_visual(target);
    }
}

fn settle_previous_motion(world: &mut World, next_target: Option<Entity>) {
    let previous = world
        .resource::<ScrollMotion>()
        .and_then(|motion| motion.target_entity);
    if previous == next_target {
        return;
    }
    if let Some(previous) = previous {
        crate::input::event::runtime::cancel_scroll(world, previous);
        settle_offset(world, previous);
    }
    if let Some(motion) = world.resource_mut::<ScrollMotion>() {
        *motion = ScrollMotion::default();
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
    if let Some(pointer) = event.pointer_identity()
        && world
            .resource::<ScrollDragState>()
            .is_some_and(|state| state.active && state.pointer != pointer)
    {
        return false;
    }
    match event {
        InputEvent::PointerDown { kind, id, x, y } => {
            super::commands::cancel_chain(world, pointer_down_target);
            if let Some(state) = world.resource_mut::<ScrollDragState>() {
                state.pointer = (*kind, *id);
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
                state.started_at_ms = now_ms;
            }
        }
        InputEvent::PointerMove { x, y, .. } => {
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
            let (target, started_at_ms) = {
                let Some(state) = world.resource::<ScrollDragState>() else {
                    return false;
                };
                (state.target, state.started_at_ms)
            };

            let dx = *x - if resolved { last_x } else { start_x };
            let dy = *y - if resolved { last_y } else { start_y };

            settle_previous_motion(world, Some(target));
            super::commands::cancel(world, target);
            let config = world.get::<ScrollConfig>(target);
            let dir = config.map(|c| c.direction).unwrap_or(ScrollAxis::Vertical);
            let elastic = config.map(|c| c.elastic).unwrap_or(true);
            let bounds = scroll_bounds(world, target).unwrap_or_default();
            let mut applied_dx = Fixed::ZERO;
            let mut applied_dy = Fixed::ZERO;
            if let Some(scroll) = world.get_mut::<ScrollOffset>(target) {
                let delta = match dir {
                    ScrollAxis::Vertical => (Fixed::ZERO, -dy),
                    ScrollAxis::Horizontal => (-dx, Fixed::ZERO),
                    ScrollAxis::Both => (-dx, -dy),
                };
                let (next_x, next_y) = displace((scroll.x, scroll.y), delta, bounds, elastic);
                applied_dx = next_x - scroll.x;
                applied_dy = next_y - scroll.y;
                scroll.x = next_x;
                scroll.y = next_y;
            }
            if let Some(motion) = world.resource_mut::<ScrollMotion>() {
                if !resolved {
                    motion.begin_pointer(target, started_at_ms);
                }
                motion.record(target, (applied_dx, applied_dy), Driver::Pointer, now_ms);
            }
            // Stuck-at-edge drags translate to zero applied delta; no
            // need to redraw a frame whose pixels didn't move.
            if applied_dx != Fixed::ZERO || applied_dy != Fixed::ZERO {
                world.invalidate_visual(target);
            }

            if let Some(state) = world.resource_mut::<ScrollDragState>() {
                state.last_x = *x;
                state.last_y = *y;
            }
        }
        InputEvent::PointerCancel { .. } | InputEvent::AppSuspend => {
            cancel_scroll_drag(world);
        }
        InputEvent::PointerUp { .. } => {
            let target_entity = {
                let Some(state) = world.resource::<ScrollDragState>() else {
                    return false;
                };
                if !state.resolved {
                    if let Some(state) = world.resource_mut::<ScrollDragState>() {
                        state.active = false;
                    }
                    return false;
                }
                state.target
            };

            let (offset_x, offset_y) = world
                .get::<ScrollOffset>(target_entity)
                .map(|s| (s.x, s.y))
                .unwrap_or((Fixed::ZERO, Fixed::ZERO));

            let bounds = scroll_bounds(world, target_entity).unwrap_or_default();
            if let Some(motion) = world.resource_mut::<ScrollMotion>() {
                motion.release_pointer(target_entity, (offset_x, offset_y), bounds, now_ms);
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
                if !input_allowed(world, target) {
                    return false;
                }
                let step = Fixed::from_int(20);
                super::commands::cancel_chain(world, Some(target));
                let offset = Fixed::from(*delta as i32) * step;
                let axis = world
                    .get::<ScrollConfig>(target)
                    .map(|c| c.direction)
                    .unwrap_or(ScrollAxis::Vertical);
                if let Some(scroll) = world.get_mut::<ScrollOffset>(target) {
                    accepted = true;
                    match axis {
                        ScrollAxis::Vertical => {
                            scroll.y -= offset;
                        }
                        ScrollAxis::Horizontal => {
                            scroll.x -= offset;
                        }
                        ScrollAxis::Both => {
                            scroll.y -= offset;
                        }
                    }
                }
                if accepted {
                    world.invalidate_visual(target);
                }
            }
        }
        InputEvent::Wheel { dx, dy, x, y } => {
            let Some(hit) = hit_test(world, root, *x, *y, screen_w, screen_h) else {
                return false;
            };
            if !input_allowed(world, hit) {
                return false;
            }
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
            let (offset_x, offset_y) = world
                .get::<ScrollOffset>(target)
                .map(|s| (s.x, s.y))
                .unwrap_or((Fixed::ZERO, Fixed::ZERO));
            let needs_x =
                matches!(axis, ScrollAxis::Horizontal | ScrollAxis::Both) && delta_x != Fixed::ZERO;
            let needs_y =
                matches!(axis, ScrollAxis::Vertical | ScrollAxis::Both) && delta_y != Fixed::ZERO;
            accepted = needs_x || needs_y;
            let (next_x, next_y) = displace(
                (offset_x, offset_y),
                (
                    if needs_x { delta_x } else { Fixed::ZERO },
                    if needs_y { delta_y } else { Fixed::ZERO },
                ),
                bounds,
                elastic,
            );
            if accepted {
                super::commands::cancel_chain(world, Some(hit));
                settle_previous_motion(world, Some(target));
                if let Some(motion) = world.resource_mut::<ScrollMotion>() {
                    motion.record(
                        target,
                        (next_x - offset_x, next_y - offset_y),
                        Driver::Stream,
                        now_ms,
                    );
                }
            }
            if let Some(offset) = world.get_mut::<ScrollOffset>(target) {
                offset.x = next_x;
                offset.y = next_y;
            }
            if next_x != offset_x || next_y != offset_y {
                world.invalidate_visual(target);
            }
        }
        _ => {}
    }
    accepted
}

/// Advances released scroll motion and completes inactive wheel streams.
pub(crate) fn scroll_inertia_system(world: &mut World) -> Result<(), crate::ecs::CommandError> {
    let Some(motion) = world.resource::<ScrollMotion>() else {
        return Ok(());
    };
    let Some(target) = motion.target_entity else {
        return Ok(());
    };
    let has_stream = motion.has_stream();
    if !has_stream
        && world
            .resource::<ScrollDragState>()
            .is_some_and(|state| state.active)
    {
        return Ok(());
    }
    let Some(offset) = world.get::<ScrollOffset>(target).copied() else {
        if let Some(motion) = world.resource_mut::<ScrollMotion>() {
            *motion = ScrollMotion::default();
        }
        return Ok(());
    };
    let bounds = scroll_bounds(world, target).unwrap_or_default();
    let elastic = world
        .get::<ScrollConfig>(target)
        .is_none_or(|config| config.elastic);
    let dt = world
        .resource::<crate::ecs::DeltaTimeMs>()
        .map_or(16, |dt| dt.0);
    let now = if has_stream { scroll_time_ms(world) } else { 0 };
    let Some(motion) = world.resource_mut::<ScrollMotion>() else {
        return Ok(());
    };
    let (next_x, next_y) = motion.tick((offset.x, offset.y), dt, bounds, elastic, now);
    if let Some(scroll) = world.get_mut::<ScrollOffset>(target) {
        scroll.x = next_x;
        scroll.y = next_y;
    }
    if next_x != offset.x || next_y != offset.y {
        world.invalidate_visual(target);
    }
    Ok(())
}

crate::ecs::system::framework_system!(pub scroll_inertia_system, ScrollInertia);

fn pointer_scroll_allowed(world: &World, start: Entity, gesture_dir: ScrollAxis) -> bool {
    if !input_allowed(world, start) {
        return false;
    }
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

fn input_allowed(world: &World, entity: Entity) -> bool {
    crate::core::reactive::retained::enabled(world, entity)
        && crate::input::event::listeners::InputBranch::scan(world, entity)
            .is_ok_and(|branch| branch.allowed())
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

#[cfg(test)]
#[path = "../../../../tests/unit/input/event/scroll/system/position_tests.rs"]
mod position_tests;
#[cfg(test)]
#[path = "../../../../tests/unit/input/event/scroll/system/tests.rs"]
mod tests;
