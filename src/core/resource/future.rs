//! Reusable pinned operation storage with exclusive polling and unwind cleanup.

use crate::core::threading::Exclusive;
use alloc::boxed::Box;
use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

pub(super) struct FutureSlot<F> {
    storage: Exclusive<Pin<Box<Option<F>>>>,
    #[cfg(feature = "sync")]
    active: bool,
}

impl<F> FutureSlot<F> {
    pub fn new() -> Self {
        Self {
            storage: Exclusive::new(Box::pin(None)),
            #[cfg(feature = "sync")]
            active: false,
        }
    }

    pub fn is_active(&self) -> bool {
        #[cfg(not(feature = "sync"))]
        {
            self.storage.get().as_ref().get_ref().is_some()
        }
        #[cfg(feature = "sync")]
        {
            self.active
        }
    }

    pub fn set(&mut self, future: Option<F>) {
        #[cfg(feature = "sync")]
        {
            // Clear before dropping the previous operation, including if its Drop unwinds.
            self.active = false;
        }
        let active = future.is_some();
        self.storage.get_mut().as_mut().set(future);
        #[cfg(feature = "sync")]
        {
            self.active = active;
        }
        #[cfg(not(feature = "sync"))]
        let _ = active;
    }
}

struct PollGuard<'a, F> {
    slot: &'a mut FutureSlot<F>,
    returned: bool,
}

impl<F> Drop for PollGuard<'_, F> {
    fn drop(&mut self) {
        if !self.returned {
            self.slot.set(None);
        }
    }
}

impl<F: Future> FutureSlot<F> {
    pub fn poll(&mut self, cx: &mut Context<'_>) -> Poll<F::Output> {
        let mut guard = PollGuard {
            slot: self,
            returned: false,
        };
        let result = guard
            .slot
            .storage
            .get_mut()
            .as_mut()
            .as_pin_mut()
            .expect("pending operation")
            .poll(cx);
        if result.is_ready() {
            guard.slot.set(None);
        }
        guard.returned = true;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::{cell::Cell, marker::PhantomPinned, task::Waker};

    struct Operation {
        address: Cell<usize>,
        fail: bool,
        _pin: PhantomPinned,
    }
    impl Future for Operation {
        type Output = ();
        fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<()> {
            let this = self.as_ref().get_ref();
            let address = this as *const Self as usize;
            let previous = this.address.replace(address);
            assert!(previous == 0 || previous == address);
            assert!(!this.fail, "operation failed");
            if previous == 0 {
                Poll::Pending
            } else {
                Poll::Ready(())
            }
        }
    }

    fn operation(fail: bool) -> Operation {
        Operation {
            address: Cell::new(0),
            fail,
            _pin: PhantomPinned,
        }
    }

    #[test]
    fn repeated_operations_preserve_pinning_and_reuse_storage_without_allocating() {
        let mut slot = FutureSlot::new();
        let mut cx = Context::from_waker(Waker::noop());
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                for _ in 0..8 {
                    slot.set(Some(operation(false)));
                    assert!(slot.is_active());
                    assert!(slot.poll(&mut cx).is_pending());
                    assert!(slot.poll(&mut cx).is_ready());
                    assert!(!slot.is_active());
                }
            }),
            0
        );
        #[cfg(not(feature = "sync"))]
        assert_eq!(
            core::mem::size_of::<FutureSlot<Operation>>(),
            core::mem::size_of::<Pin<Box<Option<Operation>>>>()
        );
    }

    #[cfg(feature = "std")]
    #[test]
    fn unwinding_clears_the_operation_before_reusing_the_slot() {
        let mut slot = FutureSlot::new();
        slot.set(Some(operation(true)));
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                || slot.poll(&mut Context::from_waker(Waker::noop()))
            ))
            .is_err()
        );
        assert!(!slot.is_active());
        slot.set(Some(operation(false)));
        assert!(
            slot.poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
    }

    #[cfg(all(feature = "std", feature = "sync"))]
    #[test]
    fn send_only_future_can_be_shared_but_is_polled_exclusively() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<FutureSlot<Operation>>();
        let mut slot = FutureSlot::new();
        slot.set(Some(operation(false)));
        let mut slot = std::thread::spawn(move || {
            assert!(
                slot.poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
            );
            slot
        })
        .join()
        .unwrap();
        assert!(
            slot.poll(&mut Context::from_waker(Waker::noop()))
                .is_ready()
        );
    }
}
