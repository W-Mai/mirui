use crate::ecs::{Entity, World};

/// The single value every `ui!` macro invocation reads from to spawn widgets.
///
/// A `UiScope` bundles the two pieces of framework state each widget needs
/// to attach itself to a running app — the ECS `World` it lives in and the
/// `Entity` it should be parented to. User code almost never constructs one
/// by hand; the `#[compose]` attribute injects it as a `cx` parameter, and
/// then the `ui!` macro reads `cx.world_mut()` / `cx.parent()` under the hood.
///
/// Later releases can grow the bundle with theme, clock, id map, or animation
/// handles without changing a single user-visible function signature — that
/// extension path is the reason the type exists.
pub struct UiScope<'w> {
    world: &'w mut World,
    parent: Entity,
}

impl<'w> UiScope<'w> {
    /// Bind a fresh `UiScope` to a specific world and parent entity.
    ///
    /// Application startup builds one of these once (typically after
    /// `spawn_root()`) and hands it to a top-level `#[compose]` function.
    /// Later widgets read through the injected `cx` parameter instead of
    /// touching this constructor themselves.
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
