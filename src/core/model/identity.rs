use super::{Model, ModelAccess};
use alloc::rc::{Rc, Weak};
use core::cell::Cell;

pub(super) struct ModelLifetime {
    generation: Cell<u64>,
    active: Cell<bool>,
}

impl ModelLifetime {
    pub(super) const fn new() -> Self {
        Self {
            generation: Cell::new(0),
            active: Cell::new(false),
        }
    }

    pub(super) fn activate(&self) -> bool {
        if self.active.get() {
            return true;
        }
        let Some(generation) = self.generation.get().checked_add(1) else {
            return false;
        };
        self.generation.set(generation);
        self.active.set(true);
        true
    }

    pub(super) fn can_activate(&self) -> bool {
        !self.active.get() && self.can_advance()
    }

    pub(super) fn can_advance(&self) -> bool {
        self.generation.get() < u64::MAX
    }

    pub(super) fn current(&self) -> Option<u64> {
        self.active.get().then(|| self.generation.get())
    }

    pub(super) fn retire(&self) {
        self.active.set(false);
    }
}

/// A weak reference to one incarnation of a registered model.
#[doc(hidden)]
pub struct ModelRef<M: Model> {
    access: Weak<dyn ModelAccess<M>>,
    generation: u64,
}

impl<M: Model> Clone for ModelRef<M> {
    fn clone(&self) -> Self {
        Self {
            access: self.access.clone(),
            generation: self.generation,
        }
    }
}

impl<M: Model> ModelRef<M> {
    #[cfg(feature = "persistence")]
    pub(crate) fn persistence_target(&self) -> Option<super::PersistenceTarget> {
        let access = self.upgrade()?;
        let owner = match access.binding_owner().0 {
            super::OwnerState::Bound(owner) => owner,
            _ => return None,
        };
        Some(super::PersistenceTarget::new(
            access.erased_identity(),
            self.generation,
            owner,
        ))
    }

    pub(super) fn new(access: &Rc<dyn ModelAccess<M>>, generation: u64) -> Self {
        Self {
            access: Rc::downgrade(access),
            generation,
        }
    }

    pub(crate) fn upgrade(&self) -> Option<Rc<dyn ModelAccess<M>>> {
        let access = self.access.upgrade()?;
        (access.generation() == Some(self.generation)).then_some(access)
    }

    pub(crate) fn callback_ready(&self) -> bool {
        use super::ModelChildren;
        let Some(access) = self.upgrade() else {
            return false;
        };
        let mut ready = false;
        access.metadata(&mut |metadata| ready = metadata.children().is_writable());
        ready
    }

    pub(crate) fn same_identity(&self, other: &Self) -> bool {
        self.generation == other.generation && Weak::ptr_eq(&self.access, &other.access)
    }

    #[cfg(all(test, feature = "std"))]
    pub(super) fn strong_count(&self) -> usize {
        self.access.strong_count()
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::ModelLifetime;

    #[test]
    fn retired_incarnations_never_become_live_again() {
        let lifetime = ModelLifetime::new();
        assert_eq!(lifetime.current(), None);
        assert!(lifetime.activate());
        let first = lifetime.current().unwrap();
        assert!(lifetime.activate());
        assert_eq!(lifetime.current(), Some(first));
        lifetime.retire();
        lifetime.retire();
        assert_eq!(lifetime.current(), None);
        assert!(lifetime.activate());
        assert!(lifetime.current().unwrap() > first);
    }

    #[test]
    fn exhausted_incarnation_cannot_wrap_or_be_reactivated() {
        let lifetime = ModelLifetime::new();
        lifetime.generation.set(u64::MAX - 1);
        assert!(lifetime.activate());
        assert_eq!(lifetime.current(), Some(u64::MAX));
        lifetime.retire();
        assert!(!lifetime.activate());
        assert_eq!(lifetime.current(), None);
        assert!(!lifetime.activate());
    }
}
