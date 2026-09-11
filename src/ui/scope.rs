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

    pub fn paths(&mut self) -> crate::text::PathAccess<'_> {
        crate::text::PathAccess::new(self.world)
    }

    pub fn bind_path(
        &mut self,
        path: crate::render::path::PathId,
        bind: impl Fn(&mut crate::render::path::Path) + 'static,
    ) -> Result<(), crate::text::PathAccessError> {
        let revision = self
            .world
            .resource::<crate::render::path::PathStore>()
            .ok_or(crate::text::PathAccessError::StoreUnavailable)?
            .revision(path)
            .map_err(crate::text::PathAccessError::from)?;
        if revision.is_none() {
            return Err(crate::text::PathAccessError::Store(
                crate::render::path::PathStoreError::Immutable(path),
            ));
        }

        let owner = self.parent;
        crate::core::reactive::with_world_scope(self.world, || {
            crate::core::reactive::effect_with_widget(owner, move || {
                crate::core::reactive::with_world(|world| {
                    if let Some(paths) = world.resource_mut::<crate::render::path::PathStore>() {
                        let _ = paths.edit(path, &bind);
                    }
                });
            });
        });
        Ok(())
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

    #[test]
    fn bind_path_rebuilds_from_signal_changes() {
        use crate::core::reactive::{Signal, flush_signal_dirty};
        use crate::render::path::{Path, PathCmd, PathStore};
        use crate::types::{Fixed, Point};

        let mut world = World::new();
        world.insert_resource(PathStore::new(1).unwrap());
        let root = world.spawn_empty();
        let path = world
            .resource_mut::<PathStore>()
            .unwrap()
            .insert(Path::new())
            .unwrap();
        let x = Signal::new(Fixed::from_int(8));
        let bound_x = x.clone();

        let mut cx = UiScope::new(&mut world, root);
        cx.bind_path(path, move |path| {
            path.clear();
            path.move_to(Point::ZERO)
                .line_to(Point::new(bound_x.get(), Fixed::ZERO));
        })
        .unwrap();

        let endpoint = world
            .resource::<PathStore>()
            .unwrap()
            .get(path)
            .unwrap()
            .commands()[1]
            .clone();
        assert_eq!(endpoint, PathCmd::LineTo(Point::new(8, 0)));

        x.set(Fixed::from_int(24));
        flush_signal_dirty(&mut world);
        let endpoint = world
            .resource::<PathStore>()
            .unwrap()
            .get(path)
            .unwrap()
            .commands()[1]
            .clone();
        assert_eq!(endpoint, PathCmd::LineTo(Point::new(24, 0)));
    }
}
