//! Registered model instances and their shared method-call boundary.

use alloc::rc::{Rc, Weak};
use core::cell::{Cell, RefCell};

use crate::ecs::world::WorldId;

/// A model type that can create its generated instance handle.
pub trait Model: Sized + 'static {
    type Handle: ModelHandle<Data = Self>;

    #[doc(hidden)]
    fn handle(cell: Weak<ModelCell<Self>>) -> Self::Handle;
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
pub struct ModelCell<M> {
    owner: WorldId,
    value: RefCell<M>,
    poisoned: Cell<bool>,
}

impl<M> ModelCell<M> {
    pub(crate) fn new(owner: WorldId, value: M) -> Self {
        Self {
            owner,
            value: RefCell::new(value),
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
        let result = update(&mut value);
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
    use super::{Model, ModelCell, ModelHandle};
    use alloc::rc::Weak;

    struct Counter(u32);

    struct CounterHandle(Weak<ModelCell<Counter>>);

    impl Clone for CounterHandle {
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }

    impl Model for Counter {
        type Handle = CounterHandle;

        fn handle(cell: Weak<ModelCell<Self>>) -> Self::Handle {
            CounterHandle(cell)
        }
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
