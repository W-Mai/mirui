//! Conditional ownership bounds for resource storage.

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
