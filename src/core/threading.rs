//! Conditional ownership bounds for resource storage.

mod state;
use core::task::Waker;
pub(crate) use state::{AccessError, State};

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
