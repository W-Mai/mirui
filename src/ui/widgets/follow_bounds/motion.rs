use super::{FollowBounds, FollowState};
use crate::anim::{Motion, Transition};
use crate::ecs::{Entity, World};
use crate::types::{Fixed, Rect};

pub(super) struct FollowMotion {
    axes: [Motion; 4],
}
impl crate::ecs::Component for FollowMotion {}

impl FollowMotion {
    fn new(rect: Rect, config: Transition) -> Self {
        Self {
            axes: [rect.x, rect.y, rect.w, rect.h].map(|value| config.motion(value, value)),
        }
    }

    fn rect(&self) -> Rect {
        Rect::new(
            self.axes[0].value(),
            self.axes[1].value(),
            self.axes[2].value().max(Fixed::ZERO),
            self.axes[3].value().max(Fixed::ZERO),
        )
    }
}

pub(super) fn prepare(world: &mut World, entity: Entity) -> bool {
    let config = world
        .get::<FollowBounds>(entity)
        .and_then(|follow| follow.transition);
    if let Some(config) = config.filter(|config| !config.is_immediate()) {
        if !world.has::<FollowMotion>(entity) {
            let rect = world
                .get::<FollowState>(entity)
                .and_then(|state| state.geometry)
                .map_or(Rect::ZERO, |geometry| geometry.rect);
            world.insert(entity, FollowMotion::new(rect, config));
            return true;
        }
    } else {
        return world.remove::<FollowMotion>(entity).is_some();
    }
    false
}

pub(super) fn redirect(world: &mut World, entity: Entity, target: Rect) -> Rect {
    let initialized = world
        .get::<FollowState>(entity)
        .is_some_and(|state| state.geometry.is_some());
    let config = world
        .get::<FollowBounds>(entity)
        .and_then(|follow| follow.transition);
    let Some((motion, config)) = world.get_mut::<FollowMotion>(entity).zip(config) else {
        return target;
    };
    if !initialized {
        *motion = FollowMotion::new(target, config);
    } else {
        for (axis, target) in motion
            .axes
            .iter_mut()
            .zip([target.x, target.y, target.w, target.h])
        {
            config.retarget(axis, target);
        }
    }
    motion.rect()
}

pub(super) fn advance(world: &mut World) -> Result<(), crate::ecs::CommandError> {
    let dt = world
        .resource::<crate::ecs::DeltaTimeMs>()
        .map_or(16, |dt| dt.0);
    world.for_each_stable::<FollowMotion>(|world, entity| {
        if !world.has::<FollowBounds>(entity)
            || crate::ui::branch::is_hidden_in_tree(world, entity)
            || !crate::core::reactive::retained::enabled(world, entity)
        {
            return;
        }
        let motion = world.get_mut::<FollowMotion>(entity).unwrap();
        if motion.axes.iter().all(Motion::is_done) {
            return;
        }
        for axis in &mut motion.axes {
            axis.tick(dt);
        }
        let rect = motion.rect();
        let Some(geometry) = world
            .get_mut::<FollowState>(entity)
            .and_then(|state| state.geometry.as_mut())
        else {
            return;
        };
        if geometry.rect != rect {
            geometry.rect = rect;
            world.mark_subtree_dirty(entity);
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests;
