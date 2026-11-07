use super::*;
use crate::anim::{Motion, Transition};

pub(super) struct ProgrammaticMotion {
    axes: [Motion; 2],
    request: Option<Request>,
    edges: [Edge; 2],
}
impl crate::ecs::Component for ProgrammaticMotion {}

fn config(world: &World, entity: Entity) -> Option<Transition> {
    world
        .get::<ScrollConfig>(entity)?
        .transition
        .filter(|config| !config.is_immediate())
}

pub(super) fn prepare(world: &mut World, entity: Entity) -> bool {
    if let Some(config) = config(world, entity) {
        if !world.has::<ProgrammaticMotion>(entity) {
            let offset = world
                .get::<ScrollOffset>(entity)
                .copied()
                .unwrap_or_default();
            world.insert(
                entity,
                ProgrammaticMotion {
                    axes: [offset.x, offset.y].map(|value| config.motion(value, value)),
                    request: None,
                    edges: [Edge::Keep; 2],
                },
            );
            return true;
        }
    } else if let Some(mut motion) = world.remove::<ProgrammaticMotion>(entity) {
        if let Some(request) = motion.request.take() {
            if let Some(pending) = world.get_mut::<Pending>(entity) {
                if pending.0.is_none() {
                    pending.0 = Some(request);
                }
            } else {
                world.insert(entity, Pending(Some(request)));
            }
            world.invalidate_visual(entity);
        }
        return true;
    }
    false
}

fn target(axis: &Motion) -> Fixed {
    match axis {
        Motion::Tween(tween) => tween.to,
        Motion::Spring(spring) => spring.target,
    }
}

pub(super) fn active(world: &World, entity: Entity) -> bool {
    world
        .get::<ProgrammaticMotion>(entity)
        .is_some_and(|motion| motion.request.is_some())
}

pub(super) fn any_active(world: &World) -> bool {
    world
        .storage::<ProgrammaticMotion>()
        .is_some_and(|storage| storage.iter().any(|(_, motion)| motion.request.is_some()))
}

pub(super) fn destination(world: &World, entity: Entity) -> Option<ScrollOffset> {
    let motion = world.get::<ProgrammaticMotion>(entity)?;
    motion.request.as_ref()?;
    Some(ScrollOffset {
        x: target(&motion.axes[0]),
        y: target(&motion.axes[1]),
    })
}

pub(super) fn cancel(world: &mut World, entity: Entity) {
    if active(world, entity) {
        world.get_mut::<ProgrammaticMotion>(entity).unwrap().request = None;
        crate::input::event::runtime::cancel_scroll(world, entity);
    }
}

pub(super) fn redirect(
    world: &mut World,
    entity: Entity,
    request: Request,
    next: ScrollOffset,
    edges: [Edge; 2],
) -> bool {
    let Some(config) = config(world, entity) else {
        return false;
    };
    let current = *world.get::<ScrollOffset>(entity).unwrap();
    super::super::system::take_input_motion(world, entity);
    let Some(motion) = world.get_mut::<ProgrammaticMotion>(entity) else {
        return false;
    };
    if motion.request.is_none() {
        motion.axes = [current.x, current.y].map(|value| config.motion(value, value));
    }
    for (axis, next) in motion.axes.iter_mut().zip([next.x, next.y]) {
        config.retarget(axis, next);
    }
    motion.edges = edges;
    motion.request = Some(request);
    true
}

fn depth_key(world: &World, entity: Entity) -> (usize, u32, u32) {
    let mut cursor = entity;
    let mut depth = 0;
    for _ in 0..world.live_entity_count() {
        let Some(parent) = world.get::<Parent>(cursor) else {
            break;
        };
        cursor = parent.0;
        depth += 1;
    }
    (depth, entity.id, entity.generation)
}

pub(super) fn reconcile(world: &mut World) -> PropertyChange {
    let mut previous = None;
    let mut change = PropertyChange::Unchanged;
    loop {
        let next = world.storage::<ProgrammaticMotion>().and_then(|storage| {
            storage
                .entities()
                .iter()
                .copied()
                .filter(|&entity| active(world, entity))
                .map(|entity| (depth_key(world, entity), entity))
                .filter(|(key, _)| previous.is_none_or(|previous| *key < previous))
                .max_by_key(|(key, _)| *key)
        });
        let Some((key, entity)) = next else { break };
        previous = Some(key);
        let next = world
            .get::<ProgrammaticMotion>(entity)
            .and_then(|motion| goal(world, entity, motion.request.as_ref()?, Some(motion.edges)));
        let Some(goal) = next else {
            cancel(world, entity);
            continue;
        };
        let next = goal.offset;
        let Some(config) = config(world, entity) else {
            continue;
        };
        let motion = world.get_mut::<ProgrammaticMotion>(entity).unwrap();
        for (index, (axis, next)) in motion.axes.iter_mut().zip([next.x, next.y]).enumerate() {
            if target(axis) != next {
                change = PropertyChange::Transform;
            }
            config.retarget(axis, next);
            let enabled = if index == 0 {
                goal.direction != ScrollAxis::Vertical
            } else {
                goal.direction != ScrollAxis::Horizontal
            };
            if enabled {
                let max = if index == 0 {
                    goal.bounds.max_x
                } else {
                    goal.bounds.max_y
                };
                let bounded = axis.value().clamp(Fixed::ZERO, max);
                if bounded != axis.value() {
                    *axis = config.motion(bounded, next);
                }
            }
        }
        let displayed = ScrollOffset {
            x: motion.axes[0].value(),
            y: motion.axes[1].value(),
        };
        if super::super::system::write_position(world, entity, displayed) {
            invalidate_for_change(world, entity, PropertyChange::Transform);
            change = PropertyChange::Transform;
        }
    }
    change
}

pub(super) fn programmatic_scroll_system(
    world: &mut World,
) -> Result<(), crate::ecs::CommandError> {
    let dt = world
        .resource::<crate::ecs::DeltaTimeMs>()
        .map_or(16, |dt| dt.0);
    world.for_each_stable::<ProgrammaticMotion>(|world, entity| {
        if !active(world, entity) {
            return;
        }
        let prepared = world
            .get::<ProgrammaticMotion>(entity)
            .and_then(|motion| goal(world, entity, motion.request.as_ref()?, Some(motion.edges)));
        let Some(prepared) = prepared else {
            cancel(world, entity);
            return;
        };
        let Some(config) = config(world, entity) else {
            return;
        };
        let bounds = prepared.bounds;
        let direction = prepared.direction;
        let actual = *world.get::<ScrollOffset>(entity).unwrap();
        let motion = world.get_mut::<ProgrammaticMotion>(entity).unwrap();
        for (index, axis) in motion.axes.iter_mut().enumerate() {
            let enabled = if index == 0 {
                direction != ScrollAxis::Vertical
            } else {
                direction != ScrollAxis::Horizontal
            };
            if !enabled {
                let value = if index == 0 { actual.x } else { actual.y };
                *axis = config.motion(value, value);
                continue;
            }
            let max = if index == 0 {
                bounds.max_x
            } else {
                bounds.max_y
            };
            let goal = target(axis).clamp(Fixed::ZERO, max);
            config.retarget(axis, goal);
            axis.tick(dt);
            let displayed = axis.value();
            let bounded = displayed.clamp(Fixed::ZERO, max);
            if displayed != bounded {
                *axis = config.motion(bounded, goal);
            }
        }
        let offset = ScrollOffset {
            x: motion.axes[0].value(),
            y: motion.axes[1].value(),
        };
        if motion.axes.iter().all(Motion::is_done) {
            motion.request = None;
        }
        if super::super::system::write_position(world, entity, offset) {
            invalidate_for_change(world, entity, PropertyChange::Transform);
        }
    });
    Ok(())
}

crate::ecs::system::framework_system!(
    pub(super) programmatic_scroll_system,
    Animation,
    expect = [ProgrammaticMotion],
);

#[cfg(test)]
mod tests;
