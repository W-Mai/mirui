//! Registered model instances and their shared method-call boundary.

use alloc::rc::{Rc, Weak};
use core::cell::{Cell, RefCell};

use crate::ecs::world::WorldId;

/// Derived observations declared by a model's inherent implementation.
#[doc(hidden)]
pub trait ModelMethods: Sized {
    type DerivedSnapshot: Copy;
    type DerivedSources: AsRef<[crate::core::reactive::ModelSource]>;

    #[doc(hidden)]
    fn derived_snapshot(&self) -> Self::DerivedSnapshot;

    #[doc(hidden)]
    fn derived_sources() -> Self::DerivedSources;

    #[doc(hidden)]
    fn publish_derived(
        sources: &Self::DerivedSources,
        before: Self::DerivedSnapshot,
        after: Self::DerivedSnapshot,
    );
}

/// A model type that can create its generated instance handle.
pub trait Model: ModelMethods + Sized + 'static {
    type Handle: ModelHandle<Data = Self>;
    type Snapshot: Copy;
    type Sources: AsRef<[crate::core::reactive::ModelSource]>;

    #[doc(hidden)]
    fn handle(cell: Weak<ModelCell<Self>>) -> Self::Handle;

    #[doc(hidden)]
    fn snapshot(&self) -> Self::Snapshot;

    #[doc(hidden)]
    fn sources() -> Self::Sources;

    #[doc(hidden)]
    fn publish(sources: &Self::Sources, before: Self::Snapshot, after: Self::Snapshot);
}

/// Maps a declaration type to the value stored by a bound callback or component.
pub trait BindType {
    type Shared: SharedValue;
}

/// A cheap, instance-preserving value accepted by generated binding captures.
pub trait SharedValue: Clone {
    fn share(&self) -> Self {
        self.clone()
    }
}

impl<T: BindType> BindType for Option<T> {
    type Shared = Option<T::Shared>;
}

impl<T: SharedValue> SharedValue for Option<T> {}

impl<T: 'static> BindType for crate::core::reactive::Signal<T> {
    type Shared = Self;
}

impl<T: 'static> SharedValue for crate::core::reactive::Signal<T> {}

impl<T: 'static> BindType for crate::core::reactive::Computed<T> {
    type Shared = Self;
}

impl<T: 'static> SharedValue for crate::core::reactive::Computed<T> {}

/// Storage owned by a model's registration entity.
#[doc(hidden)]
pub struct ModelCell<M: Model> {
    owner: WorldId,
    value: RefCell<M>,
    sources: M::Sources,
    derived_sources: M::DerivedSources,
    poisoned: Cell<bool>,
}

impl<M: Model> ModelCell<M> {
    pub(crate) fn new(owner: WorldId, value: M) -> Self {
        Self {
            owner,
            value: RefCell::new(value),
            sources: M::sources(),
            derived_sources: M::derived_sources(),
            poisoned: Cell::new(false),
        }
    }

    pub(crate) fn owner(&self) -> WorldId {
        self.owner
    }
}

struct UpdateGuard<'a> {
    poisoned: &'a Cell<bool>,
    committed: bool,
}

impl Drop for UpdateGuard<'_> {
    fn drop(&mut self) {
        if !self.committed {
            self.poisoned.set(true);
        }
    }
}

/// A handle to one registered model instance.
pub trait ModelHandle: Clone {
    type Data: Model<Handle = Self>;

    #[doc(hidden)]
    fn cell(&self) -> &Weak<ModelCell<Self::Data>>;

    #[doc(hidden)]
    fn read<R>(&self, read: impl FnOnce(&Self::Data) -> R) -> R {
        let cell = self
            .cell()
            .upgrade()
            .expect("model registration is no longer alive");
        if let Some(active) = crate::core::reactive::current_world_id() {
            assert_eq!(active, cell.owner(), "model belongs to a different App");
        }
        assert!(
            !cell.poisoned.get(),
            "model was poisoned by a failed update"
        );
        let value = cell
            .value
            .try_borrow()
            .expect("model is already mutably borrowed");
        read(&value)
    }

    #[doc(hidden)]
    fn read_observed<R>(&self, index: usize, read: impl FnOnce(&Self::Data) -> R) -> R {
        let cell = self
            .cell()
            .upgrade()
            .expect("model registration is no longer alive");
        self.read(|value| {
            cell.sources.as_ref()[index].track();
            read(value)
        })
    }

    #[doc(hidden)]
    fn read_derived<R>(&self, index: usize, read: impl FnOnce(&Self::Data) -> R) -> R {
        let cell = self
            .cell()
            .upgrade()
            .expect("model registration is no longer alive");
        self.read(|value| {
            cell.derived_sources.as_ref()[index].track();
            read(value)
        })
    }

    #[doc(hidden)]
    fn update<R>(&self, update: impl FnOnce(&mut Self::Data) -> R) -> R {
        let cell = self
            .cell()
            .upgrade()
            .expect("model registration is no longer alive");
        if let Some(active) = crate::core::reactive::current_world_id() {
            assert_eq!(active, cell.owner(), "model belongs to a different App");
        }
        assert!(
            !cell.poisoned.get(),
            "model was poisoned by a failed update"
        );
        let mut value = cell
            .value
            .try_borrow_mut()
            .expect("model is already borrowed");
        let mut guard = UpdateGuard {
            poisoned: &cell.poisoned,
            committed: false,
        };
        let before = value.snapshot();
        let derived_before = value.derived_snapshot();
        let result = update(&mut value);
        let after = value.snapshot();
        let derived_after = value.derived_snapshot();
        drop(value);
        Self::Data::publish(&cell.sources, before, after);
        Self::Data::publish_derived(&cell.derived_sources, derived_before, derived_after);
        guard.committed = true;
        result
    }
}

pub(crate) fn register<M: Model>(owner: WorldId, value: M) -> (Rc<ModelCell<M>>, M::Handle) {
    let cell = Rc::new(ModelCell::new(owner, value));
    let handle = M::handle(Rc::downgrade(&cell));
    (cell, handle)
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::{Model, ModelCell, ModelHandle, ModelMethods};
    use alloc::rc::Weak;

    struct Counter(u32);

    struct CounterHandle(Weak<ModelCell<Counter>>);

    impl ModelMethods for Counter {
        type DerivedSnapshot = ();
        type DerivedSources = [crate::core::reactive::ModelSource; 0];

        fn derived_snapshot(&self) -> Self::DerivedSnapshot {}

        fn derived_sources() -> Self::DerivedSources {
            []
        }

        fn publish_derived(
            _: &Self::DerivedSources,
            _: Self::DerivedSnapshot,
            _: Self::DerivedSnapshot,
        ) {
        }
    }

    impl Clone for CounterHandle {
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }

    impl Model for Counter {
        type Handle = CounterHandle;
        type Snapshot = ();
        type Sources = [crate::core::reactive::ModelSource; 0];

        fn handle(cell: Weak<ModelCell<Self>>) -> Self::Handle {
            CounterHandle(cell)
        }

        fn snapshot(&self) -> Self::Snapshot {}

        fn sources() -> Self::Sources {
            []
        }

        fn publish(_: &Self::Sources, _: Self::Snapshot, _: Self::Snapshot) {}
    }

    impl ModelHandle for CounterHandle {
        type Data = Counter;

        fn cell(&self) -> &Weak<ModelCell<Counter>> {
            &self.0
        }
    }

    impl CounterHandle {
        fn increment(&self) {
            self.update(|counter| counter.0 += 1);
        }

        fn value(&self) -> u32 {
            self.read(|counter| counter.0)
        }
    }

    #[test]
    fn registered_instances_are_independent_and_clones_share_one_instance() {
        let mut app = crate::app::App::headless(32, 32);
        let first = app.add_model(Counter(1));
        let second = app.add_model(Counter(8));
        let first_copy = first.clone();

        first.increment();
        first_copy.increment();
        second.increment();

        assert_eq!(first.value(), 3);
        assert_eq!(first_copy.value(), 3);
        assert_eq!(second.value(), 9);
    }

    #[test]
    fn handle_rejects_access_after_app_drops() {
        let handle = {
            let mut app = crate::app::App::headless(32, 32);
            app.add_model(Counter(4))
        };
        let error = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handle.value()));
        assert!(error.is_err());
    }

    #[test]
    fn nested_write_is_rejected_and_failed_update_poisoned() {
        let mut app = crate::app::App::headless(32, 32);
        let handle = app.add_model(Counter(1));
        let nested = handle.clone();
        let error = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            handle.update(|counter| {
                counter.0 = 2;
                nested.increment();
            });
        }));
        assert!(error.is_err());
        let read_after_failure =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handle.value()));
        assert!(read_after_failure.is_err());
    }

    #[test]
    fn active_other_app_cannot_access_a_model() {
        let mut first_app = crate::app::App::headless(32, 32);
        let first = first_app.add_model(Counter(1));
        let mut second_app = crate::app::App::headless(32, 32);
        let error = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = crate::core::reactive::OwnerGuard::enter(&mut second_app.world);
            first.increment();
        }));
        assert!(error.is_err());
        assert_eq!(first.value(), 1);
    }
}
