//! Short-lived resource capabilities used by generated composition code.

use ::core::any::type_name;
use ::core::marker::PhantomData;

use crate::core::reactive::{OwnerGuard, WorldAccessGuard};
use crate::ecs::World;
use crate::ecs::resource::{ResourceMut, ResourceRef};

struct Access<'scope> {
    world: &'scope mut World,
    _suppression: WorldAccessGuard,
    _owner: OwnerGuard,
}

impl<'scope> Access<'scope> {
    fn new(world: &'scope mut World) -> Self {
        let owner = OwnerGuard::enter(world);
        let suppression = WorldAccessGuard::suppress(world);
        Self {
            world,
            _suppression: suppression,
            _owner: owner,
        }
    }
}

impl Drop for Access<'_> {
    fn drop(&mut self) {
        // Keep the World loan alive until its suppression and owner guards drop.
    }
}

/// Read one required resource within a short composition borrow.
#[doc(hidden)]
pub struct ResourceRead<'scope, T: 'static> {
    access: Access<'scope>,
    _type: PhantomData<fn() -> T>,
}

impl<'scope, T: 'static> ResourceRead<'scope, T> {
    pub(crate) fn new(world: &'scope mut World) -> Self {
        Self {
            access: Access::new(world),
            _type: PhantomData,
        }
    }

    pub fn with<R>(&self, read: impl for<'resource> FnOnce(ResourceRef<'resource, T>) -> R) -> R {
        let value = self
            .access
            .world
            .resource::<T>()
            .unwrap_or_else(|| panic!("composition requires resource `{}`", type_name::<T>()));
        read(ResourceRef::new(value, self.access.world.id()))
    }
}

/// Read an optional resource without exposing a reference outside the closure.
#[doc(hidden)]
pub struct OptionalResourceRead<'scope, T: 'static> {
    access: Access<'scope>,
    _type: PhantomData<fn() -> T>,
}

impl<'scope, T: 'static> OptionalResourceRead<'scope, T> {
    pub(crate) fn new(world: &'scope mut World) -> Self {
        Self {
            access: Access::new(world),
            _type: PhantomData,
        }
    }

    pub fn with<R>(
        &self,
        read: impl for<'resource> FnOnce(Option<ResourceRef<'resource, T>>) -> R,
    ) -> R {
        read(
            self.access
                .world
                .resource::<T>()
                .map(|value| ResourceRef::new(value, self.access.world.id())),
        )
    }
}

/// Mutate one required resource within a short composition borrow.
#[doc(hidden)]
pub struct ResourceWrite<'scope, T: 'static> {
    access: Access<'scope>,
    _type: PhantomData<fn() -> T>,
}

impl<'scope, T: 'static> ResourceWrite<'scope, T> {
    pub(crate) fn new(world: &'scope mut World) -> Self {
        Self {
            access: Access::new(world),
            _type: PhantomData,
        }
    }

    pub fn with<R>(
        &mut self,
        write: impl for<'resource> FnOnce(ResourceMut<'resource, T>) -> R,
    ) -> R {
        let owner = self.access.world.id();
        let value = self
            .access
            .world
            .resource_mut::<T>()
            .unwrap_or_else(|| panic!("composition requires resource `{}`", type_name::<T>()));
        write(ResourceMut::new(value, owner))
    }
}

/// Install and remove owned resources immediately during composition.
#[doc(hidden)]
pub struct Resources<'scope> {
    access: Access<'scope>,
}

impl<'scope> Resources<'scope> {
    pub(crate) fn new(world: &'scope mut World) -> Self {
        Self {
            access: Access::new(world),
        }
    }

    pub fn insert<T: 'static>(&mut self, value: T) {
        self.access.world.insert_resource(value);
    }

    pub fn remove<T: 'static>(&mut self) -> Option<T> {
        self.access.world.remove_resource::<T>()
    }
}

#[cfg(test)]
mod tests {
    use ::core::cell::Cell;
    use alloc::boxed::Box;
    use alloc::rc::Rc;

    use crate::core::reactive::{Effect, Signal, current_world_id, with_world, with_world_scope};
    use crate::ecs::World;
    use crate::ui::UiScope;

    #[crate::model]
    struct Counter {
        #[observe]
        value: u32,
    }

    #[crate::model]
    impl Counter {
        fn increment(&mut self) {
            self.value += 1;
        }
    }

    struct DropObserver(Rc<Cell<u32>>);

    impl Drop for DropObserver {
        fn drop(&mut self) {
            assert!(with_world(|_| ()).is_none());
            self.0.set(self.0.get() + 1);
        }
    }

    #[test]
    fn typed_resource_access_is_immediate_and_scoped() {
        let mut world = World::new();
        world.insert_resource(3_u32);
        let owner = world.id();
        let root = world.spawn_empty();
        let mut cx = UiScope::new(&mut world, root);
        assert_eq!(cx.__res_read::<u32>().with(|value| *value), 3);
        assert_eq!(cx.__res_read::<u32>().with(|value| value.owner), owner);
        assert_eq!(cx.__res_write::<u32>().with(|value| value.owner), owner);
        assert_eq!(
            cx.__res_optional::<u32>()
                .with(|value| value.unwrap().owner),
            owner
        );
        assert!(cx.__res_optional::<u64>().with(|value| value.is_none()));
        cx.__res_write::<u32>().with(|mut value| *value += 4);
        assert_eq!(cx.__res_read::<u32>().with(|value| *value), 7);
        cx.__resources().insert(11_u32);
        assert_eq!(cx.__resources().remove::<u32>(), Some(11));
        assert_eq!(cx.__resources().remove::<u32>(), None);
        let external = alloc::string::String::from("external");
        assert_eq!(
            cx.__res_optional::<u32>().with(|_| external.as_str()),
            "external"
        );
    }

    #[test]
    #[should_panic(expected = "requires resource")]
    fn required_resource_access_rejects_missing_values() {
        let mut world = World::new();
        let root = world.spawn_empty();
        UiScope::new(&mut world, root)
            .__res_read::<u32>()
            .with(|_| ());
    }

    #[test]
    fn resource_borrow_suppresses_only_the_matching_world() {
        let mut first = World::new();
        first.insert_resource(7_u32);
        let root = first.spawn_empty();
        let mut second = World::new();
        let second_id = second.id();
        with_world_scope(&mut first, || {
            with_world(|world| {
                let mut cx = UiScope::new(world, root);
                cx.__res_read::<u32>().with(|value| {
                    assert_eq!(*value, 7);
                    assert!(with_world(|_| ()).is_none());
                    with_world_scope(&mut second, || {
                        assert_eq!(with_world(|world| world.id()), Some(second_id));
                    });
                    assert!(with_world(|_| ()).is_none());
                });
                let reentrant = std::panic::catch_unwind(|| with_world(|_| ()));
                assert!(reentrant.is_err());
            })
            .unwrap();
            assert!(with_world(|_| ()).is_some());
        });
    }

    #[test]
    fn resource_suppression_covers_arguments_replacement_and_unwind() {
        let drops = Rc::new(Cell::new(0));
        let mut world = World::new();
        world.insert_resource(DropObserver(Rc::clone(&drops)));
        world.insert_resource(3_u32);
        let root = world.spawn_empty();
        with_world_scope(&mut world, || {
            with_world(|world| {
                let mut cx = UiScope::new(world, root);
                cx.__resources().insert({
                    assert!(with_world(|_| ()).is_none());
                    DropObserver(Rc::clone(&drops))
                });
                assert_eq!(drops.get(), 1);
                let mut resources = cx.__resources();
                let removed = resources.remove::<DropObserver>().expect("owned resource");
                assert_eq!(drops.get(), 1);
                drop(removed);
                assert_eq!(drops.get(), 2);
                drop(resources);
                let fail_argument = Cell::new(true);
                let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    cx.__resources().insert(if fail_argument.get() {
                        assert!(with_world(|_| ()).is_none());
                        panic!("resource argument unwind");
                    } else {
                        42_u32
                    });
                }));
                assert!(failed.is_err());
                assert_eq!(cx.__res_read::<u32>().with(|value| *value), 3);
                let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    cx.__res_write::<u32>().with(|mut value| {
                        *value = 9;
                        assert!(with_world(|_| ()).is_none());
                        panic!("resource unwind");
                    });
                }));
                assert!(failed.is_err());
                assert_eq!(cx.__res_read::<u32>().with(|value| *value), 9);
                let reentrant = std::panic::catch_unwind(|| with_world(|_| ()));
                assert!(reentrant.is_err());
            })
            .unwrap();
            assert!(with_world(|_| ()).is_some());
        });
        drop(world);
        assert_eq!(drops.get(), 2);
    }

    #[test]
    fn resource_access_preserves_model_signal_and_effect_behavior() {
        let mut app = crate::app::App::headless(1, 1);
        let model = app.add_model(Counter { value: 0 }).finish();
        app.add_resource(3_u32);
        let root = app.spawn_root().id();
        let owner = app.world.id();
        let signal = Signal::new(0_u32);
        let runs = Rc::new(Cell::new(0));
        app.compose(root, |cx| {
            cx.__res_write::<u32>().with(|mut value| {
                assert_eq!(current_world_id(), Some(owner));
                model.increment();
                signal.set(5);
                *value += signal.get();
                let observed = signal.clone();
                let effect_runs = Rc::clone(&runs);
                let effect = Effect::new(move || {
                    let _ = observed.get();
                    effect_runs.set(effect_runs.get() + 1);
                });
                assert_eq!(runs.get(), 1);
                drop(effect);
            });
        });
        assert_eq!(model.value(), 1);
        assert_eq!(signal.get(), 5);
        assert_eq!(app.world.resource::<u32>(), Some(&8));
    }

    #[test]
    fn resource_access_uses_its_scope_owner_and_restores_the_outer_owner() {
        let mut first = crate::app::App::headless(1, 1);
        let model = first.add_model(Counter { value: 0 }).finish();
        first.add_resource(3_u32);
        let first_root = first.spawn_root().id();
        let first_owner = first.world.id();
        let mut second = crate::app::App::headless(1, 1);
        let second_root = second.spawn_root().id();
        let second_owner = second.world.id();
        let mut first_scope = UiScope::new(&mut first.world, first_root);
        let _second_scope = UiScope::new(&mut second.world, second_root);
        assert_eq!(current_world_id(), Some(second_owner));
        first_scope.__res_read::<u32>().with(|value| {
            assert_eq!(*value, 3);
            assert_eq!(current_world_id(), Some(first_owner));
            model.increment();
        });
        assert_eq!(current_world_id(), Some(second_owner));
    }

    #[test]
    fn resource_facades_cannot_revive_an_escaped_scope_window() {
        let mut anchor = World::new();
        let mut first = World::new();
        let mut second = World::new();
        let root = second.spawn_empty();
        let second_ref = &mut second;
        with_world_scope(&mut anchor, || {
            let mut scope = with_world_scope(&mut first, move || UiScope::new(second_ref, root));
            let moved_first = Box::new(first);
            scope.__resources().insert(7_u32);
            scope.__res_read::<u32>().with(|value| {
                assert_eq!(*value, 7);
                assert!(with_world(|_| ()).is_none());
            });
            drop(scope);
            assert!(with_world(|_| ()).is_none());
            drop(moved_first);
        });
        assert!(with_world(|_| ()).is_none());
    }

    #[test]
    fn prepared_resource_capabilities_do_not_allocate() {
        let mut world = World::new();
        world.insert_resource(3_u32);
        let root = world.spawn_empty();
        let mut cx = UiScope::new(&mut world, root);
        let allocations = crate::test_support::tracked_allocations(|| {
            for value in 0_u32..64 {
                cx.__resources().insert(value);
                cx.__res_write::<u32>().with(|mut value| *value += 1);
                assert_eq!(cx.__res_read::<u32>().with(|value| *value), value + 1);
                assert!(cx.__res_optional::<u64>().with(|value| value.is_none()));
            }
        });
        assert_eq!(allocations, 0);
    }
}
