use crate::ecs::{Entity, World};

/// A construction scope supplied by `App::compose`.
///
/// Use `spawn` for components and typed widget builders, `children` for nested composition, and `on_event` for callbacks on completed nodes. `#[compose]` and `ui!` use the same construction lifecycle without an explicit scope parameter in author code.
pub struct UiScope<'w> {
    world: &'w mut World,
    parent: Entity,
    _owner_scope: crate::core::reactive::OwnerGuard,
}

impl<'w> UiScope<'w> {
    /// Construct a child from static components and typed widget builders.
    ///
    /// Tuples insert their static components in order. Parent and ID registration precede View attachment, initial property bindings, and event installation. Every node receives default `Widget`, `Style`, and `Children` components.
    pub fn spawn(&mut self, recipe: impl super::recipe::WidgetRecipe) -> Entity {
        use super::recipe::PropertyBindings;
        use crate::input::event::binding::EntityEventBindings;

        let entity = super::construction::spawn_widget(self.world);
        let (properties, events) = recipe.insert(self.world, entity);
        super::construction::finish_widget_static(self.world, self.parent, entity);
        properties.install(self.world, entity);
        events.install(self.world, entity);
        entity
    }

    /// Construct a child container and return its entity together with the child composition's result.
    pub fn children<R>(
        &mut self,
        recipe: impl super::recipe::WidgetRecipe,
        build: impl FnOnce(&mut UiScope<'_>) -> R,
    ) -> (Entity, R) {
        let entity = self.spawn(recipe);
        let output = build(&mut self.with_parent(entity));
        (entity, output)
    }

    /// Install a callback on a constructed node, validating its attached event source.
    pub fn on_event<E>(
        &mut self,
        entity: Entity,
        key: crate::input::event::contract::EventKey<E>,
        callback: crate::__macro_support::EventCallback<E, crate::__macro_support::NodeContext>,
    ) where
        E: crate::input::event::contract::Event,
        E::Route: crate::__macro_support::EntityInstallRoute<E>,
        E::Source: crate::input::event::contract::EntityEventSource,
        <E::Route as crate::input::event::contract::EventRoute>::Decision:
            crate::__macro_support::ListenerDecision,
    {
        crate::input::event::binding::install_entity_event(self.world, entity, key, callback);
    }

    /// Bind a fresh `UiScope` to a specific world and parent entity.
    ///
    /// `App::compose` creates the scope for a prepared composition. Generated widget code uses the scope during that execution window.
    pub fn new(world: &'w mut World, parent: Entity) -> Self {
        let scope = crate::core::reactive::OwnerGuard::enter(world);
        Self {
            world,
            parent,
            _owner_scope: scope,
        }
    }

    #[doc(hidden)]
    pub fn assert_binding<T: crate::core::model::SharedValue>(&mut self, value: &T) {
        let _owner = crate::core::reactive::OwnerGuard::enter(self.world);
        value.assert_registered_owner();
    }

    #[doc(hidden)]
    pub fn __res_read<T: 'static>(&mut self) -> super::resource_access::ResourceRead<'_, T> {
        super::resource_access::ResourceRead::new(self.world)
    }

    #[doc(hidden)]
    pub fn __res_optional<T: 'static>(
        &mut self,
    ) -> super::resource_access::OptionalResourceRead<'_, T> {
        super::resource_access::OptionalResourceRead::new(self.world)
    }

    #[doc(hidden)]
    pub fn __res_write<T: 'static>(&mut self) -> super::resource_access::ResourceWrite<'_, T> {
        super::resource_access::ResourceWrite::new(self.world)
    }

    #[doc(hidden)]
    pub fn __resources(&mut self) -> super::resource_access::Resources<'_> {
        super::resource_access::Resources::new(self.world)
    }

    pub(crate) fn world_mut(&mut self) -> &mut World {
        self.world
    }

    #[doc(hidden)]
    pub fn parent(&self) -> Entity {
        self.parent
    }

    pub fn paths(&mut self) -> crate::text::PathAccess<'_> {
        crate::text::PathAccess::new(self.world)
    }

    /// Read a component during composition without subscribing to changes.
    ///
    /// The reference is limited to this scope borrow. Use a bound model or an
    /// explicit reactive binding when later changes must update the UI.
    pub fn component<T: crate::ecs::Component>(
        &self,
        entity: impl core::borrow::Borrow<Entity>,
    ) -> Option<&T> {
        self.world.get::<T>(*entity.borrow())
    }

    pub fn bind_property<P: crate::ui::property::Property>(
        &mut self,
        entity: Entity,
        read: impl Fn() -> P::Value + 'static,
    ) {
        super::recipe::bind_property::<P>(self.world, entity, read);
    }

    pub fn bind_path(
        &mut self,
        path: crate::render::path::PathId,
        bind: impl Fn(&mut crate::render::path::Path) + 'static,
    ) -> Result<(), crate::text::PathAccessError> {
        self.bind_path_visual(self.parent, path, bind)
    }

    /// Rebuilds a mutable path from reactive inputs and invalidates the widget
    /// that paints the path without forcing a layout pass.
    pub fn bind_path_visual(
        &mut self,
        target: Entity,
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

        crate::core::reactive::with_world_scope(self.world, || {
            crate::core::reactive::effect_with_widget(target, move || {
                crate::core::reactive::with_world(|world| {
                    if let Some(paths) = world.resource_mut::<crate::render::path::PathStore>() {
                        let _ = paths.edit(path, &bind);
                    }
                    world.invalidate_visual(target);
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
        let scope = crate::core::reactive::OwnerGuard::enter(self.world);
        UiScope {
            world: self.world,
            parent: new_parent,
            _owner_scope: scope,
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
        let world_id = world.id();
        let mut cx = UiScope::new(&mut world, parent);
        assert_eq!(crate::core::reactive::current_world_id(), Some(world_id));
        assert!(crate::core::reactive::with_world(|_| ()).is_none());
        assert_eq!(cx.parent(), parent);
        let child = cx.world_mut().spawn_empty();
        assert!(cx.world_mut().is_alive(child));
        drop(cx);
        assert_eq!(crate::core::reactive::current_world_id(), None);
    }

    #[test]
    fn owner_scope_restores_after_composition_unwinds() {
        let mut world = World::new();
        let parent = world.spawn_empty();
        let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _scope = UiScope::new(&mut world, parent);
            panic!("composition unwind");
        }));
        assert!(failed.is_err());
        assert_eq!(crate::core::reactive::current_world_id(), None);
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
        assert!(world.has::<crate::ui::dirty::VisualDirty>(root));
    }

    #[test]
    fn custom_property_binding_is_typed_and_reactive() {
        use crate::core::reactive::{Signal, flush_signal_dirty};
        use crate::ui::property::Property;
        use crate::ui::{Style, Widget};

        struct BorderWidth;

        impl Property for BorderWidth {
            type Value = crate::types::Fixed;

            fn apply(
                world: &mut World,
                entity: Entity,
                value: Self::Value,
            ) -> crate::ui::property::PropertyChange {
                world.get_mut::<Style>(entity).unwrap().border_width = value;
                crate::ui::property::PropertyChange::Visual
            }
        }

        let mut world = World::new();
        let root = world.spawn_empty();
        let widget = world.spawn_empty();
        world.insert(widget, Widget);
        world.insert(widget, Style::default());
        let width = Signal::new(crate::types::Fixed::from_int(2));
        let bound_width = width.clone();

        UiScope::new(&mut world, root)
            .bind_property::<BorderWidth>(widget, move || bound_width.get());
        assert_eq!(
            world.get::<Style>(widget).unwrap().border_width,
            crate::types::Fixed::from_int(2)
        );

        width.set(crate::types::Fixed::from_int(4));
        flush_signal_dirty(&mut world);
        assert_eq!(
            world.get::<Style>(widget).unwrap().border_width,
            crate::types::Fixed::from_int(4)
        );
    }
}
