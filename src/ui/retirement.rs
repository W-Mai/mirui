use crate::ecs::commands::{CommandError, DeferredCommands};
use crate::ecs::{Entity, World};

use super::{Children, IdMap, NamedId, Parent, WidgetRoot};

impl DeferredCommands<'_> {
    /// Delete the target and its attached descendants after the callback returns. An active App root and its ancestors cannot be deleted.
    #[doc(hidden)]
    pub fn despawn(&mut self, entity: Entity) {
        self.apply_empty(entity, apply_despawn);
    }
}

fn apply_despawn(
    world: &mut World,
    target: Option<Entity>,
    _payload: *mut u8,
    _event: Option<&dyn core::any::Any>,
) -> Result<(), CommandError> {
    despawn_subtree(world, target.expect("despawn command target"));
    Ok(())
}

fn contains_active_root(world: &World, target: Entity) -> bool {
    let Some(WidgetRoot(mut current)) = world.resource::<WidgetRoot>().copied() else {
        return false;
    };
    for _ in 0..world.live_entity_count() {
        if current == target {
            return true;
        }
        let Some(parent) = world.get::<Parent>(current).map(|parent| parent.0) else {
            break;
        };
        if !world
            .get::<Children>(parent)
            .is_some_and(|children| children.0.contains(&current))
        {
            break;
        }
        current = parent;
    }
    false
}

pub(crate) fn despawn_subtree(world: &mut World, entity: Entity) {
    if !world.is_alive(entity) {
        return;
    }
    if contains_active_root(world, entity) {
        crate::warn!(target: "mirui::ui", "cannot despawn the active App root or its ancestors");
        return;
    }

    if let Some(parent) = world.get::<Parent>(entity).map(|parent| parent.0) {
        if let Some(children) = world.get_mut::<Children>(parent) {
            children.0.retain(|child| *child != entity);
        }
        world.invalidate(parent);
    }

    let mut current = entity;
    loop {
        let child = world
            .get_mut::<Children>(current)
            .and_then(|children| children.0.pop());
        if let Some(child) = child {
            if world
                .get::<Parent>(child)
                .is_some_and(|parent| parent.0 == current)
            {
                current = child;
            }
            continue;
        }

        let parent = world.get::<Parent>(current).map(|parent| parent.0);
        if let Some(rect) = world
            .get::<super::dirty::PaintedCoverage>(current)
            .map(|rect| rect.0.outer)
            && rect.w > crate::types::Fixed::ZERO
            && rect.h > crate::types::Fixed::ZERO
        {
            world.invalidate_rect(rect);
        }
        if let Some(name) = world.get::<NamedId>(current).map(|name| name.0)
            && let Some(map) = world.resource_mut::<IdMap>()
            && map.get(name) == Some(current)
        {
            map.remove(name);
        }
        crate::core::reactive::cleanup_effects_for(world, current);
        world.despawn(current);
        if current == entity {
            break;
        }
        current = parent.expect("attached child retains its parent until retirement");
    }
}

#[cfg(test)]
mod tests;
