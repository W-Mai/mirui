use super::components::{ScrollAxis, ScrollConfig, ScrollDelta, ScrollOffset, TouchAction};
use super::controller::{Driver, ScrollMotion, displace};
use crate::ecs::{Entity, World};
use crate::input::event::hit_test::hit_test;
use crate::input::event::input::InputEvent;
use crate::types::{Fixed, Fixed64};

use super::bounds::scroll_bounds;

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

fn settle_offset(world: &mut World, target: Entity) {
    let bounds = scroll_bounds(world, target).unwrap_or_default();
    let Some(offset) = world.get_mut::<ScrollOffset>(target) else {
        return;
    };
    let old = (offset.x, offset.y);
    offset.x = offset.x.clamp(Fixed::ZERO, bounds.max_x);
    offset.y = offset.y.clamp(Fixed::ZERO, bounds.max_y);
    let delta = (offset.x - old.0, offset.y - old.1);
    accumulate_scroll_delta(world, target, delta.0, delta.1);
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
            accumulate_scroll_delta(world, target, applied_dx, applied_dy);
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
                accumulate_scroll_delta(world, target, next_x - offset_x, next_y - offset_y);
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
        accumulate_scroll_delta(world, target, next_x - offset.x, next_y - offset.y);
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
mod tests {
    use super::*;

    #[test]
    fn wheel_and_rotary_respect_ancestor_input_locks() {
        let (mut world, root) = fallback_scroll_world();
        let owner = world.spawn(crate::input::event::listeners::InputBlocked);
        world.insert(root, crate::ui::Parent(owner));
        for event in [
            InputEvent::Wheel {
                dx: Fixed::ZERO,
                dy: Fixed::from_int(-20),
                x: 20.into(),
                y: 20.into(),
            },
            InputEvent::Rotary { id: 0, delta: -1 },
        ] {
            assert!(!scroll_system_with_target(
                &mut world,
                root,
                &event,
                100,
                100,
                Some(root),
                0
            ));
            assert_eq!(world.get::<ScrollOffset>(root).unwrap().y, Fixed::ZERO);
        }
        world.remove::<crate::input::event::listeners::InputBlocked>(owner);
        assert!(scroll_system_with_target(
            &mut world,
            root,
            &InputEvent::Rotary { id: 0, delta: -1 },
            100,
            100,
            None,
            16
        ));
        assert!(world.get::<ScrollOffset>(root).unwrap().y > Fixed::ZERO);
    }

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
        world.insert_resource(ScrollMotion::default());
        crate::ui::render_system::update_layout(
            &mut world,
            root,
            &Viewport::new(100, 100, Fixed::ONE),
        );
        world.invalidate_visual(root);
        (world, root)
    }

    #[test]
    fn a_click_without_dragging_preserves_the_released_scroll_motion() {
        let (mut world, root) = fallback_scroll_world();
        world.get_mut::<ScrollConfig>(root).unwrap().elastic = true;
        world.insert_resource(crate::ecs::DeltaTimeMs(8));
        for (event, hit, now) in [
            (
                InputEvent::PointerDown {
                    kind: crate::input::event::input::PointerKind::Mouse,
                    id: 0,
                    x: 20.into(),
                    y: 20.into(),
                },
                Some(root),
                0,
            ),
            (
                InputEvent::PointerMove {
                    kind: crate::input::event::input::PointerKind::Mouse,
                    id: 0,
                    x: 20.into(),
                    y: 40.into(),
                },
                None,
                8,
            ),
            (
                InputEvent::PointerUp {
                    kind: crate::input::event::input::PointerKind::Mouse,
                    id: 0,
                    x: 20.into(),
                    y: 40.into(),
                },
                None,
                8,
            ),
        ] {
            scroll_system_with_target(&mut world, root, &event, 100, 100, hit, now);
        }
        let stretched = world.get::<ScrollOffset>(root).unwrap().y;
        assert!(stretched < Fixed::ZERO);
        assert!(world.resource::<ScrollMotion>().unwrap().y.is_some());

        let button = world.spawn_empty();
        for (event, hit, now) in [
            (
                InputEvent::PointerDown {
                    kind: crate::input::event::input::PointerKind::Mouse,
                    id: 0,
                    x: 60.into(),
                    y: 60.into(),
                },
                Some(button),
                16,
            ),
            (
                InputEvent::PointerUp {
                    kind: crate::input::event::input::PointerKind::Mouse,
                    id: 0,
                    x: 60.into(),
                    y: 60.into(),
                },
                None,
                24,
            ),
        ] {
            scroll_system_with_target(&mut world, root, &event, 100, 100, hit, now);
            let motion = world.resource::<ScrollMotion>().unwrap();
            assert_eq!(motion.target_entity, Some(root));
            assert!(motion.y.is_some());
            assert_eq!(world.get::<ScrollOffset>(root).unwrap().y, stretched);
        }
        for _ in 0..500 {
            scroll_inertia_system(&mut world).unwrap();
        }
        assert_eq!(world.get::<ScrollOffset>(root).unwrap().y, Fixed::ZERO);
        assert!(
            world
                .resource::<ScrollMotion>()
                .unwrap()
                .target_entity
                .is_none()
        );
    }

    #[test]
    fn other_pointer_categories_cannot_take_over_or_release_scroll_capture() {
        use crate::surface::PointerKind;
        let (mut world, root) = fallback_scroll_world();
        scroll_system_with_target(
            &mut world,
            root,
            &InputEvent::PointerDown {
                kind: PointerKind::Mouse,
                id: 0,
                x: 20.into(),
                y: 20.into(),
            },
            100,
            100,
            Some(root),
            0,
        );
        scroll_system_with_target(
            &mut world,
            root,
            &InputEvent::PointerMove {
                kind: PointerKind::Mouse,
                id: 0,
                x: 20.into(),
                y: 5.into(),
            },
            100,
            100,
            None,
            10,
        );
        let offset = world.get::<ScrollOffset>(root).unwrap().y;
        assert!(offset > Fixed::ZERO);
        for event in [
            InputEvent::PointerLeave {
                kind: PointerKind::Mouse,
                id: 0,
            },
            InputEvent::PointerEnter {
                kind: PointerKind::Mouse,
                id: 0,
            },
        ] {
            assert!(!scroll_system_with_target(
                &mut world, root, &event, 100, 100, None, 15
            ));
            assert!(world.resource::<ScrollDragState>().unwrap().active);
            assert!(world.resource::<ScrollDragState>().unwrap().resolved);
            assert_eq!(world.get::<ScrollOffset>(root).unwrap().y, offset);
        }
        for kind in [PointerKind::Touch, PointerKind::Pen, PointerKind::Unknown] {
            for event in [
                InputEvent::PointerDown {
                    kind,
                    id: 0,
                    x: 90.into(),
                    y: 90.into(),
                },
                InputEvent::PointerMove {
                    kind,
                    id: 0,
                    x: 90.into(),
                    y: 90.into(),
                },
                InputEvent::PointerUp {
                    kind,
                    id: 0,
                    x: 90.into(),
                    y: 90.into(),
                },
                InputEvent::PointerCancel {
                    kind,
                    id: 0,
                    x: 90.into(),
                    y: 90.into(),
                },
            ] {
                scroll_system_with_target(&mut world, root, &event, 100, 100, Some(root), 20);
                let state = world.resource::<ScrollDragState>().unwrap();
                assert!(state.active && state.resolved);
                assert_eq!(state.pointer, (PointerKind::Mouse, 0));
                assert_eq!(world.get::<ScrollOffset>(root).unwrap().y, offset);
            }
        }
    }

    #[test]
    fn target_handoff_and_cancellation_leave_no_abandoned_stretch() {
        let (mut world, root) = fallback_scroll_world();
        world.get_mut::<ScrollConfig>(root).unwrap().elastic = true;
        let other = world.spawn(ScrollOffset::default());
        world.insert(
            other,
            crate::ui::ComputedRect(crate::types::Rect::new(0, 0, 100, 100)),
        );
        world.insert(
            other,
            ScrollConfig {
                content_height: 300.into(),
                ..ScrollConfig::default()
            },
        );
        for (target, now) in [(root, 0), (other, 16)] {
            scroll_system_with_target(
                &mut world,
                root,
                &InputEvent::PointerDown {
                    kind: crate::input::event::input::PointerKind::Mouse,
                    id: 0,
                    x: 20.into(),
                    y: 20.into(),
                },
                100,
                100,
                Some(target),
                now,
            );
            scroll_system_with_target(
                &mut world,
                root,
                &InputEvent::PointerMove {
                    kind: crate::input::event::input::PointerKind::Mouse,
                    id: 0,
                    x: 20.into(),
                    y: 40.into(),
                },
                100,
                100,
                None,
                now + 8,
            );
            assert!(world.get::<ScrollOffset>(target).unwrap().y < Fixed::ZERO);
            assert_eq!(
                world.resource::<ScrollMotion>().unwrap().target_entity,
                Some(target)
            );
        }
        assert_eq!(world.get::<ScrollOffset>(root).unwrap().y, Fixed::ZERO);
        scroll_system_with_target(
            &mut world,
            root,
            &InputEvent::PointerCancel {
                kind: crate::input::event::input::PointerKind::Mouse,
                id: 0,
                x: 20.into(),
                y: 40.into(),
            },
            100,
            100,
            None,
            32,
        );
        assert_eq!(world.get::<ScrollOffset>(other).unwrap().y, Fixed::ZERO);
        assert!(
            world
                .resource::<ScrollMotion>()
                .unwrap()
                .target_entity
                .is_none()
        );
        assert!(!world.resource::<ScrollDragState>().unwrap().active);
    }

    #[test]
    fn pointer_release_cannot_end_another_targets_wheel_stream() {
        let _clock = crate::core::time::mock::install();
        crate::core::time::mock::set_ms(0);
        let (mut world, root) = fallback_scroll_world();
        world.get_mut::<ScrollConfig>(root).unwrap().elastic = true;
        let pointer_target = world.spawn(ScrollOffset {
            x: Fixed::ZERO,
            y: 70.into(),
        });
        world.insert(
            pointer_target,
            crate::ui::ComputedRect(crate::types::Rect::new(0, 0, 100, 100)),
        );
        world.insert(
            pointer_target,
            ScrollConfig {
                content_height: 300.into(),
                ..ScrollConfig::default()
            },
        );
        scroll_system_with_target(
            &mut world,
            root,
            &InputEvent::PointerDown {
                kind: crate::input::event::input::PointerKind::Mouse,
                id: 0,
                x: 20.into(),
                y: 70.into(),
            },
            100,
            100,
            Some(pointer_target),
            0,
        );
        scroll_system_with_target(
            &mut world,
            root,
            &InputEvent::PointerMove {
                kind: crate::input::event::input::PointerKind::Mouse,
                id: 0,
                x: 20.into(),
                y: 60.into(),
            },
            100,
            100,
            None,
            8,
        );
        assert!(scroll_system_with_target(
            &mut world,
            root,
            &InputEvent::Wheel {
                dx: Fixed::ZERO,
                dy: 8.into(),
                x: 20.into(),
                y: 20.into(),
            },
            100,
            100,
            None,
            16,
        ));
        let stretched = world.get::<ScrollOffset>(root).unwrap().y;
        assert!(stretched < Fixed::ZERO);
        scroll_system_with_target(
            &mut world,
            root,
            &InputEvent::PointerUp {
                kind: crate::input::event::input::PointerKind::Mouse,
                id: 0,
                x: 20.into(),
                y: 60.into(),
            },
            100,
            100,
            None,
            24,
        );
        let motion = world.resource::<ScrollMotion>().unwrap();
        assert_eq!(motion.target_entity, Some(root));
        assert!(motion.has_stream());
        assert!(motion.x.is_none() && motion.y.is_none());
        crate::core::time::mock::set_ms(32);
        scroll_inertia_system(&mut world).unwrap();
        assert_eq!(world.get::<ScrollOffset>(root).unwrap().y, stretched);
        assert_eq!(
            world.get::<ScrollOffset>(pointer_target).unwrap().y,
            80.into()
        );
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
        assert!(world.resource::<ScrollMotion>().unwrap().y.is_none());

        world.get_mut::<ScrollOffset>(root).unwrap().y = Fixed::from_int(200);
        world.insert_resource(ScrollMotion::default());
        assert!(scroll_system_with_target(
            &mut world, root, &event, 100, 100, None, 0
        ));
        assert_eq!(
            world.get::<ScrollOffset>(root).unwrap().y,
            Fixed::from_int(200)
        );
        assert!(world.resource::<ScrollMotion>().unwrap().y.is_none());
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
                .resource::<ScrollMotion>()
                .unwrap()
                .target_entity
                .is_none()
        );
        world.remove_resource::<ScrollMotion>();
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
                    kind: crate::input::event::input::PointerKind::Mouse,
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
                        kind: crate::input::event::input::PointerKind::Mouse,
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
                    kind: crate::input::event::input::PointerKind::Mouse,
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
                world.resource::<ScrollMotion>().unwrap().y.is_some(),
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
    fn continuous_wheel_input_owns_position_until_the_stream_ends_without_allocating() {
        let _clock = crate::core::time::mock::install();
        for interval in [8, 16, 32] {
            for start in [0, 24] {
                crate::core::time::mock::set_ms(0);
                let (mut world, root) = fallback_scroll_world();
                world.get_mut::<ScrollConfig>(root).unwrap().elastic = true;
                world.get_mut::<ScrollOffset>(root).unwrap().y = Fixed::from_int(start);
                world.insert_resource(crate::ecs::DeltaTimeMs(interval as u16));
                let event = InputEvent::Wheel {
                    dx: Fixed::ZERO,
                    dy: Fixed::from_int(8),
                    x: 20.into(),
                    y: 20.into(),
                };
                assert_eq!(
                    crate::test_support::tracked_allocations(|| {
                        for frame in 0..16 {
                            crate::core::time::mock::set_ms(frame * interval);
                            let before = world.get::<ScrollOffset>(root).unwrap().y;
                            scroll_system(&mut world, root, &event, 100, 100);
                            let after = world.get::<ScrollOffset>(root).unwrap().y;
                            assert!(after < before);
                            scroll_inertia_system(&mut world).unwrap();
                            assert_eq!(world.get::<ScrollOffset>(root).unwrap().y, after);
                        }
                        assert!(world.get::<ScrollOffset>(root).unwrap().y < Fixed::ZERO);
                        for frame in 16..400 {
                            crate::core::time::mock::set_ms(frame * interval);
                            scroll_inertia_system(&mut world).unwrap();
                            assert!(world.get::<ScrollOffset>(root).unwrap().y <= Fixed::ZERO);
                        }
                    }),
                    0
                );
                assert_eq!(world.get::<ScrollOffset>(root).unwrap().y, Fixed::ZERO);
                assert!(
                    world
                        .resource::<ScrollMotion>()
                        .unwrap()
                        .target_entity
                        .is_none()
                );
                crate::core::time::mock::set_ms(50000);
                scroll_system(&mut world, root, &event, 100, 100);
                assert!(world.get::<ScrollOffset>(root).unwrap().y < Fixed::ZERO);
                world.get_mut::<ScrollConfig>(root).unwrap().elastic = false;
                scroll_inertia_system(&mut world).unwrap();
                assert_eq!(world.get::<ScrollOffset>(root).unwrap().y, Fixed::ZERO);
            }
        }
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
            kind: crate::input::event::input::PointerKind::Mouse,
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
            state.pointer = (crate::surface::PointerKind::Mouse, 0);
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
                kind: crate::input::event::input::PointerKind::Mouse,
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
        world.insert_resource(ScrollMotion::default());
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
        if let Some(state) = world.resource_mut::<ScrollDragState>() {
            state.pointer = (crate::surface::PointerKind::Mouse, 0);
            state.active = true;
            state.resolved = true;
            state.target = target;
            state.last_resolved_target = Some(target);
        }
        let motion = world.resource_mut::<ScrollMotion>().unwrap();
        motion.begin_pointer(target, 0);
        motion.record(target, (Fixed::ZERO, (-10).into()), Driver::Pointer, 10);
        let event = InputEvent::PointerUp {
            kind: crate::input::event::input::PointerKind::Mouse,
            id: 0,
            x: Fixed::from_int(64),
            y: Fixed::from_int(50),
        };
        scroll_system_with_target(&mut world, target, &event, 128, 128, None, 10);
        assert!(world.resource::<ScrollMotion>().unwrap().y.is_some());
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
        world.insert_resource(ScrollMotion::default());
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
            state.pointer = (crate::surface::PointerKind::Mouse, 0);
            state.active = true;
            state.resolved = true;
            state.target = target;
        }

        scroll_system(
            &mut world,
            target,
            &InputEvent::PointerCancel {
                kind: crate::input::event::input::PointerKind::Mouse,
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
        let spring = world.resource::<ScrollMotion>().unwrap();
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
        world.insert_resource(ScrollMotion::default());
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
            state.pointer = (crate::surface::PointerKind::Mouse, 0);
            state.active = true;
            state.resolved = true;
            state.target = target;
            state.last_resolved_target = Some(target);
        }
        let motion = world.resource_mut::<ScrollMotion>().unwrap();
        motion.begin_pointer(target, 0);
        motion.record(target, (Fixed::ZERO, 10.into()), Driver::Pointer, 10);
        scroll_system_with_target(
            &mut world,
            target,
            &InputEvent::PointerUp {
                kind: crate::input::event::input::PointerKind::Mouse,
                id: 0,
                x: Fixed::from_int(64),
                y: Fixed::from_int(50),
            },
            128,
            100,
            None,
            10,
        );
        assert!(world.resource::<ScrollMotion>().unwrap().y.is_some());
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
                kind: crate::input::event::input::PointerKind::Mouse,
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
                    kind: crate::input::event::input::PointerKind::Mouse,
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
