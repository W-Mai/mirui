use super::entity::Entity;
use super::world::World;

/// A typed widget builder that defers entity creation: it accumulates
/// configuration at compile time, then unfolds into individual component
/// inserts the moment it is spawned. Implementors own their satellite
/// components (style, handlers) and insert each in [`spawn_into`].
///
/// [`spawn_into`]: IntoBundle::spawn_into
pub trait IntoBundle {
    fn spawn_into(self, world: &mut World, entity: Entity);
}

/// Opt-in marker for types that are a single component. Gates the blanket
/// `IntoBundle` so builders (which impl `IntoBundle` directly) stay coherent.
pub trait Component: 'static {}

impl<C: Component> IntoBundle for C {
    fn spawn_into(self, world: &mut World, entity: Entity) {
        world.insert(entity, self);
    }
}

macro_rules! impl_bundle_for_tuple {
    ($($name:ident),+) => {
        impl<$($name: IntoBundle),+> IntoBundle for ($($name,)+) {
            fn spawn_into(self, world: &mut World, entity: Entity) {
                #[allow(non_snake_case)]
                let ($($name,)+) = self;
                $($name.spawn_into(world, entity);)+
            }
        }
    };
}

impl_bundle_for_tuple!(A);
impl_bundle_for_tuple!(A, B);
impl_bundle_for_tuple!(A, B, C);
impl_bundle_for_tuple!(A, B, C, D);
impl_bundle_for_tuple!(A, B, C, D, E);
impl_bundle_for_tuple!(A, B, C, D, E, F);
impl_bundle_for_tuple!(A, B, C, D, E, F, G);
impl_bundle_for_tuple!(A, B, C, D, E, F, G, H);

impl World {
    /// Allocate an entity and let `bundle` unfold its components onto it.
    pub fn spawn<B: IntoBundle>(&mut self, bundle: B) -> Entity {
        let entity = self.spawn_empty();
        self.insert(entity, crate::ui::Widget);
        bundle.spawn_into(self, entity);
        entity
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Foo(u32);
    struct Bar;
    impl Component for Foo {}
    impl Component for Bar {}
    struct FooBuilder {
        foo: Foo,
        bar: Option<Bar>,
    }
    impl IntoBundle for FooBuilder {
        fn spawn_into(self, world: &mut World, entity: Entity) {
            world.insert(entity, self.foo);
            if let Some(bar) = self.bar {
                world.insert(entity, bar);
            }
        }
    }

    #[test]
    fn spawn_single_component() {
        let mut world = World::default();
        let e = world.spawn(Foo(7));
        assert_eq!(world.get::<Foo>(e).unwrap().0, 7);
    }

    #[test]
    fn spawn_tuple_of_components() {
        let mut world = World::default();
        let e = world.spawn((Foo(3), Bar));
        assert_eq!(world.get::<Foo>(e).unwrap().0, 3);
        assert!(world.has::<Bar>(e));
    }

    #[test]
    fn spawn_tuple_mixing_builder_and_component() {
        let mut world = World::default();
        let e = world.spawn((
            FooBuilder {
                foo: Foo(9),
                bar: Some(Bar),
            },
            Foo(11),
        ));
        assert_eq!(world.get::<Foo>(e).unwrap().0, 11);
        assert!(world.has::<Bar>(e));
    }

    #[test]
    fn spawn_unfolds_components() {
        let mut world = World::default();
        let e = world.spawn(FooBuilder {
            foo: Foo(42),
            bar: Some(Bar),
        });
        assert_eq!(world.get::<Foo>(e).unwrap().0, 42);
        assert!(world.has::<Bar>(e));
    }

    #[test]
    fn optional_satellite_absent() {
        let mut world = World::default();
        let e = world.spawn(FooBuilder {
            foo: Foo(1),
            bar: None,
        });
        assert!(world.has::<Foo>(e));
        assert!(!world.has::<Bar>(e));
    }
}
