//! Conditional ownership bounds for resource storage.

mod state;
use core::task::Waker;
pub(crate) use state::{AccessError, State};

/// Short metadata access. Callers move owned payloads out before running external code.
pub(crate) struct Gate<T> {
    #[cfg(not(feature = "sync-cache"))]
    value: core::cell::RefCell<T>,
    #[cfg(feature = "sync-cache")]
    value: critical_section::Mutex<core::cell::RefCell<T>>,
}

impl<T> Gate<T> {
    pub(crate) fn new(value: T) -> Self {
        let value = core::cell::RefCell::new(value);
        Self {
            #[cfg(not(feature = "sync-cache"))]
            value,
            #[cfg(feature = "sync-cache")]
            value: critical_section::Mutex::new(value),
        }
    }

    pub(crate) fn with<R>(&self, body: impl FnOnce(&mut T) -> R) -> Result<R, AccessError> {
        #[cfg(not(feature = "sync-cache"))]
        {
            self.value
                .try_borrow_mut()
                .map(|mut value| body(&mut value))
                .map_err(|_| AccessError::Reentrant)
        }
        #[cfg(feature = "sync-cache")]
        {
            critical_section::with(|cs| {
                self.value
                    .borrow(cs)
                    .try_borrow_mut()
                    .map(|mut value| body(&mut value))
                    .map_err(|_| AccessError::Reentrant)
            })
        }
    }

    pub(crate) fn get_mut(&mut self) -> &mut T {
        #[cfg(not(feature = "sync-cache"))]
        {
            self.value.get_mut()
        }
        #[cfg(feature = "sync-cache")]
        {
            self.value.get_mut().get_mut()
        }
    }
}

/// State accessed only through an exclusive owner borrow, never through a shared reference.
pub(crate) struct Exclusive<T> {
    #[cfg(not(feature = "sync-cache"))]
    value: T,
    #[cfg(feature = "sync-cache")]
    value: critical_section::Mutex<core::cell::Cell<T>>,
}

impl<T> Exclusive<T> {
    pub(crate) fn new(value: T) -> Self {
        Self {
            #[cfg(not(feature = "sync-cache"))]
            value,
            #[cfg(feature = "sync-cache")]
            value: critical_section::Mutex::new(core::cell::Cell::new(value)),
        }
    }

    #[cfg(not(feature = "sync-cache"))]
    pub(crate) fn get(&self) -> &T {
        &self.value
    }

    pub(crate) fn get_mut(&mut self) -> &mut T {
        #[cfg(not(feature = "sync-cache"))]
        {
            &mut self.value
        }
        #[cfg(feature = "sync-cache")]
        {
            self.value.get_mut().get_mut()
        }
    }
}

#[cfg(not(feature = "sync-cache"))]
pub(crate) use alloc::rc::Rc as Shared;
#[cfg(feature = "sync-cache")]
pub(crate) use alloc::sync::Arc as Shared;

#[cfg(all(feature = "sync-cache", not(target_has_atomic = "ptr")))]
compile_error!("shared resource storage requires pointer-width atomic operations");

/// Conditional transfer bound used by storage callbacks.
#[cfg(feature = "sync-cache")]
pub trait Transfer: Send {}
#[cfg(feature = "sync-cache")]
impl<T: Send + ?Sized> Transfer for T {}

/// Conditional transfer bound used by storage callbacks.
#[cfg(not(feature = "sync-cache"))]
pub trait Transfer {}
#[cfg(not(feature = "sync-cache"))]
impl<T: ?Sized> Transfer for T {}

/// Conditional bound for immutable resource owners.
#[cfg(feature = "sync-cache")]
pub trait Share: Send + Sync {}
#[cfg(feature = "sync-cache")]
impl<T: Send + Sync + ?Sized> Share for T {}

/// Conditional bound for immutable resource owners.
#[cfg(not(feature = "sync-cache"))]
pub trait Share {}
#[cfg(not(feature = "sync-cache"))]
impl<T: ?Sized> Share for T {}

/// Execute one bounded storage transaction without changing the local path.
#[inline]
pub(crate) fn transaction<R>(body: impl FnOnce() -> R) -> R {
    #[cfg(feature = "sync-cache")]
    {
        critical_section::with(|_| body())
    }
    #[cfg(not(feature = "sync-cache"))]
    {
        body()
    }
}

/// Move an already cloned waiter into bounded storage without invoking its vtable.
pub(crate) fn insert_waiter<const N: usize>(
    slots: &mut [Option<Waker>; N],
    incoming: &mut Option<Waker>,
) -> bool {
    if slots
        .iter()
        .flatten()
        .any(|old| old.will_wake(incoming.as_ref().expect("incoming waiter")))
    {
        return true;
    }
    if let Some(slot) = slots.iter_mut().find(|slot| slot.is_none()) {
        *slot = incoming.take();
        return true;
    }
    false
}

#[cfg(all(test, feature = "sync-cache"))]
mod tests {
    use super::*;

    #[test]
    fn shared_bounds_reject_local_owners_and_nontransferable_operations() {
        trait AmbiguousShare<A> {
            fn check() {}
        }
        impl<T: ?Sized> AmbiguousShare<()> for T {}
        struct SharedBound;
        impl<T: Share + ?Sized> AmbiguousShare<SharedBound> for T {}
        let _ = <alloc::rc::Rc<()> as AmbiguousShare<_>>::check;
        let _ = <core::cell::Cell<u32> as AmbiguousShare<_>>::check;

        trait AmbiguousTransfer<A> {
            fn check() {}
        }
        impl<T: ?Sized> AmbiguousTransfer<()> for T {}
        struct TransferBound;
        impl<T: Transfer + ?Sized> AmbiguousTransfer<TransferBound> for T {}
        let _ = <core::future::Ready<alloc::rc::Rc<()>> as AmbiguousTransfer<_>>::check;
        let _ = <state::Lease<'_, u32> as AmbiguousTransfer<_>>::check;
    }
}
