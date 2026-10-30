use core::cell::RefCell;
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};
#[cfg(feature = "sync")]
use core::task::Waker;
use core::task::{Context, Poll};

#[cfg(all(feature = "sync", feature = "std"))]
type Owner = std::thread::ThreadId;
#[cfg(not(all(feature = "sync", feature = "std")))]
#[derive(Clone, Copy)]
struct Owner;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AccessError {
    Busy,
    Reentrant,
    #[cfg(feature = "sync")]
    Capacity,
}

struct Contents<T> {
    value: Option<T>,
    #[cfg(feature = "sync")]
    waiters: [Option<Waker>; 8],
    #[cfg(feature = "sync")]
    has_waiters: bool,
    #[cfg(all(feature = "sync", feature = "std"))]
    owner: Option<Owner>,
}

/// Transfer a prepared state out of its gate before calling external code.
pub(crate) struct State<T> {
    inner: super::Gate<Contents<T>>,
}

fn current_owner() -> Owner {
    #[cfg(all(feature = "sync", feature = "std"))]
    {
        std::thread::current().id()
    }
    #[cfg(not(all(feature = "sync", feature = "std")))]
    {
        Owner
    }
}

impl<T> State<T> {
    pub(crate) const EXTRA_BYTES: usize =
        core::mem::size_of::<Self>() - core::mem::size_of::<RefCell<Option<T>>>();

    pub(crate) fn new(value: T) -> Self {
        let inner = super::Gate::new(Contents {
            value: Some(value),
            #[cfg(feature = "sync")]
            waiters: core::array::from_fn(|_| None),
            #[cfg(feature = "sync")]
            has_waiters: false,
            #[cfg(all(feature = "sync", feature = "std"))]
            owner: None,
        });
        Self { inner }
    }

    fn metadata<R>(&self, body: impl FnOnce(&mut Contents<T>) -> R) -> Result<R, AccessError> {
        self.inner.with(body)
    }

    fn take(state: &mut Contents<T>, _owner: Owner) -> Result<T, AccessError> {
        let Some(value) = state.value.take() else {
            #[cfg(all(feature = "sync", feature = "std"))]
            if state.owner == Some(_owner) {
                return Err(AccessError::Reentrant);
            }
            return Err(AccessError::Busy);
        };
        #[cfg(all(feature = "sync", feature = "std"))]
        {
            state.owner = Some(_owner);
        }
        Ok(value)
    }

    pub(crate) fn try_take(&self) -> Result<Lease<'_, T>, AccessError> {
        let owner = current_owner();
        let value = self.metadata(|state| Self::take(state, owner))??;
        Ok(Lease {
            state: self,
            value: Some(value),
            _local: PhantomData,
        })
    }

    pub(crate) fn poll_take(
        &self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Lease<'_, T>, AccessError>> {
        match self.try_take() {
            Err(AccessError::Busy) => {}
            result => return Poll::Ready(result),
        }
        #[cfg(not(feature = "sync"))]
        {
            let _ = cx;
            Poll::Ready(Err(AccessError::Busy))
        }
        #[cfg(feature = "sync")]
        {
            let owner = current_owner();
            let mut incoming = Some(cx.waker().clone());
            let value = self.metadata(|state| match Self::take(state, owner) {
                Ok(value) => Ok(Some(value)),
                Err(AccessError::Busy) => {
                    if super::insert_waiter(&mut state.waiters, &mut incoming) {
                        state.has_waiters = true;
                        Ok(None)
                    } else {
                        Err(AccessError::Capacity)
                    }
                }
                Err(error) => Err(error),
            })??;
            match value {
                Some(value) => Poll::Ready(Ok(Lease {
                    state: self,
                    value: Some(value),
                    _local: PhantomData,
                })),
                None => Poll::Pending,
            }
        }
    }

    pub(crate) fn try_with<R>(&self, body: impl FnOnce(&mut T) -> R) -> Result<R, AccessError> {
        #[cfg(not(feature = "sync"))]
        {
            self.metadata(|state| state.value.as_mut().map(body).ok_or(AccessError::Busy))?
        }
        #[cfg(feature = "sync")]
        {
            let mut lease = self.try_take()?;
            Ok(body(&mut lease))
        }
    }

    pub(crate) fn get_mut(&mut self) -> &mut T {
        self.inner
            .get_mut()
            .value
            .as_mut()
            .expect("exclusive state")
    }

    #[cfg(test)]
    pub(crate) fn metadata_available(&self) -> bool {
        self.metadata(|_| ()).is_ok()
    }
}

pub(crate) struct Lease<'a, T> {
    state: &'a State<T>,
    value: Option<T>,
    _local: PhantomData<*mut ()>,
}

impl<T> Deref for Lease<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.value.as_ref().expect("active state")
    }
}

impl<T> DerefMut for Lease<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.value.as_mut().expect("active state")
    }
}

impl<T> Drop for Lease<'_, T> {
    fn drop(&mut self) {
        let value = self.value.take();
        let wake = self
            .state
            .metadata(|state| {
                let old = core::mem::replace(&mut state.value, value);
                #[cfg(all(feature = "sync", feature = "std"))]
                {
                    state.owner = None;
                }
                #[cfg(feature = "sync")]
                {
                    let wake = state.has_waiters.then(|| {
                        state.has_waiters = false;
                        core::mem::replace(&mut state.waiters, core::array::from_fn(|_| None))
                    });
                    (old, wake)
                }
                #[cfg(not(feature = "sync"))]
                {
                    (old, ())
                }
            })
            .expect("return state outside its metadata gate");
        assert!(wake.0.is_none(), "state returned twice");
        #[cfg(feature = "sync")]
        if let Some(wakers) = wake.1 {
            for parent in wakers.into_iter().flatten() {
                parent.wake();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_work_does_not_hold_metadata_and_warm_access_does_not_allocate() {
        let state = State::new(1_u32);
        let allocations = crate::test_support::tracked_allocations(|| {
            let mut lease = state.try_take().unwrap();
            assert!(state.metadata_available());
            assert!(state.try_take().is_err());
            *lease = 2;
            drop(lease);
            assert_eq!(state.try_with(|value| *value).unwrap(), 2);
        });
        assert_eq!(allocations, 0);
        #[cfg(not(feature = "sync"))]
        assert_eq!(State::<u32>::EXTRA_BYTES, 0);
    }

    #[cfg(feature = "std")]
    #[test]
    fn unwinding_restores_the_state_without_rolling_back_completed_changes() {
        let state = State::new(1_u32);
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut lease = state.try_take().unwrap();
                *lease = 2;
                panic!("external operation failed");
            }))
            .is_err()
        );
        assert_eq!(state.try_with(|value| *value).unwrap(), 2);
    }

    #[cfg(all(feature = "std", feature = "sync"))]
    #[test]
    fn bounded_waiters_deduplicate_and_wake_after_restoring_the_state() {
        use alloc::sync::{Arc, Weak};
        use alloc::task::Wake;
        use core::sync::atomic::{AtomicUsize, Ordering};

        struct Observer {
            state: Weak<State<u32>>,
            calls: AtomicUsize,
        }
        impl Wake for Observer {
            fn wake(self: Arc<Self>) {
                self.wake_by_ref();
            }
            fn wake_by_ref(self: &Arc<Self>) {
                let state = self.state.upgrade().unwrap();
                assert_eq!(state.try_with(|value| *value).unwrap(), 7);
                self.calls.fetch_add(1, Ordering::Relaxed);
            }
        }

        let state = Arc::new(State::new(7));
        let observers: [_; 9] = core::array::from_fn(|_| {
            Arc::new(Observer {
                state: Arc::downgrade(&state),
                calls: AtomicUsize::new(0),
            })
        });
        let lease = state.try_take().unwrap();
        std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    for observer in &observers[..8] {
                        let waker = Waker::from(observer.clone());
                        let mut cx = Context::from_waker(&waker);
                        assert!(state.poll_take(&mut cx).is_pending());
                        assert!(state.poll_take(&mut cx).is_pending());
                    }
                    let waker = Waker::from(observers[8].clone());
                    let mut cx = Context::from_waker(&waker);
                    assert!(matches!(
                        state.poll_take(&mut cx),
                        Poll::Ready(Err(AccessError::Capacity))
                    ));
                })
                .join()
                .unwrap();
        });
        drop(lease);
        for observer in &observers[..8] {
            assert_eq!(observer.calls.load(Ordering::Relaxed), 1);
        }
        assert_eq!(observers[8].calls.load(Ordering::Relaxed), 0);
    }
}
