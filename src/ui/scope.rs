use crate::ecs::{Entity, World};

pub struct UiScope<'w> {
    world: &'w mut World,
    parent: Entity,
}

impl<'w> UiScope<'w> {
    pub fn new(world: &'w mut World, parent: Entity) -> Self {
        Self { world, parent }
    }

    #[doc(hidden)]
    pub fn world_mut(&mut self) -> &mut World {
        self.world
    }

    #[doc(hidden)]
    pub fn parent(&self) -> Entity {
        self.parent
    }

    #[doc(hidden)]
    pub fn with_parent<'a>(&'a mut self, new_parent: Entity) -> UiScope<'a>
    where
        'w: 'a,
    {
        UiScope {
            world: self.world,
            parent: new_parent,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_holds_world_and_parent() {
        let mut world = World::new();
        let parent = world.spawn_empty();
        let mut cx = UiScope::new(&mut world, parent);
        assert_eq!(cx.parent(), parent);
        let child = cx.world_mut().spawn_empty();
        assert!(cx.world_mut().is_alive(child));
    }

    #[test]
    fn with_parent_shifts_target() {
        let mut world = World::new();
        let root = world.spawn_empty();
        let other = world.spawn_empty();
        let mut cx = UiScope::new(&mut world, root);
        assert_eq!(cx.parent(), root);
        {
            let inner = cx.with_parent(other);
            assert_eq!(inner.parent(), other);
        }
        assert_eq!(cx.parent(), root);
    }
}
