//! Resource readiness shared with I/O completion threads and interrupt handlers.
//! Notification never polls a loader or touches reactive state.

use alloc::boxed::Box;
use core::{
    cell::RefCell,
    task::{RawWaker, RawWakerVTable, Waker},
};

struct WakeData {
    owners: usize,
    ready: bool,
    parent: Option<Waker>,
}

struct WakeState(critical_section::Mutex<RefCell<WakeData>>);

pub(crate) struct TaskWake {
    waker: Waker,
}

impl TaskWake {
    pub fn new() -> Self {
        let state = Box::new(WakeState(critical_section::Mutex::new(RefCell::new(
            WakeData {
                owners: 1,
                ready: true,
                parent: None,
            },
        ))));
        let raw = RawWaker::new(Box::into_raw(state).cast(), &VTABLE);
        // SAFETY: the vtable owns the boxed state and synchronizes every reference-count and readiness access.
        Self {
            waker: unsafe { Waker::from_raw(raw) },
        }
    }

    fn state(&self) -> &WakeState {
        // SAFETY: construction is private and this Waker retains one strong state reference.
        unsafe { &*self.waker.data().cast::<WakeState>() }
    }

    pub fn take_ready(&self) -> bool {
        critical_section::with(|cs| {
            core::mem::replace(&mut self.state().0.borrow(cs).borrow_mut().ready, false)
        })
    }

    pub fn register(&self, parent: &Waker) -> &Waker {
        if parent.will_wake(&self.waker) {
            return &self.waker;
        }
        let parent = parent.clone();
        let (old, notify) = critical_section::with(|cs| {
            let mut data = self.state().0.borrow(cs).borrow_mut();
            let old = data.parent.replace(parent);
            let notify = data.ready.then(|| data.parent.take()).flatten();
            (old, notify)
        });
        // Arbitrary parent vtables run outside the critical section and outside every RefCell borrow.
        drop(old);
        if let Some(parent) = notify {
            parent.wake();
        }
        &self.waker
    }

    pub fn schedule(&self) {
        self.waker.wake_by_ref();
    }

    pub fn waker(&self) -> &Waker {
        &self.waker
    }
}

static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, release);

unsafe fn clone(data: *const ()) -> RawWaker {
    // SAFETY: every vtable invocation holds a live reference until it returns.
    let state = unsafe { &*data.cast::<WakeState>() };
    critical_section::with(|cs| {
        let mut state = state.0.borrow(cs).borrow_mut();
        state.owners = state
            .owners
            .checked_add(1)
            .expect("resource waker reference overflow");
    });
    RawWaker::new(data, &VTABLE)
}

unsafe fn release(data: *const ()) {
    // SAFETY: this invocation consumes exactly one counted reference.
    let state = unsafe { &*data.cast::<WakeState>() };
    let last = critical_section::with(|cs| {
        let mut state = state.0.borrow(cs).borrow_mut();
        state.owners -= 1;
        state.owners == 0
    });
    if last {
        // SAFETY: the final reference exclusively owns the original Box allocation; parent destruction runs unlocked.
        drop(unsafe { Box::from_raw(data.cast_mut().cast::<WakeState>()) });
    }
}

unsafe fn wake_by_ref(data: *const ()) {
    // SAFETY: wake_by_ref borrows an existing counted reference.
    let state = unsafe { &*data.cast::<WakeState>() };
    let parent = critical_section::with(|cs| {
        let mut state = state.0.borrow(cs).borrow_mut();
        state.ready = true;
        state.parent.take()
    });
    if let Some(parent) = parent {
        parent.wake();
    }
}

unsafe fn wake(data: *const ()) {
    struct Release(*const ());
    impl Drop for Release {
        fn drop(&mut self) {
            // SAFETY: this guard owns the reference consumed by wake, including during unwinding.
            unsafe { release(self.0) };
        }
    }
    let _release = Release(data);
    // SAFETY: the guard retains the reference throughout notification.
    unsafe { wake_by_ref(data) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{sync::Arc, task::Wake};
    use core::sync::atomic::{AtomicUsize, Ordering};

    struct Parent(AtomicUsize);
    impl Wake for Parent {
        fn wake(self: Arc<Self>) {
            self.wake_by_ref();
        }
        fn wake_by_ref(self: &Arc<Self>) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[test]
    fn wake_registration_handles_both_race_orders_and_coalesces_notifications() {
        let task = TaskWake::new();
        let parent = Arc::new(Parent(AtomicUsize::new(0)));
        let host: Waker = parent.clone().into();
        assert!(task.take_ready());
        assert!(!task.take_ready());
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                let io = task.register(&host).clone();
                io.wake_by_ref();
                io.clone().wake();
                assert_eq!(parent.0.load(Ordering::Relaxed), 1);
                assert!(task.take_ready());
                task.schedule();
                task.register(&host);
                assert_eq!(parent.0.load(Ordering::Relaxed), 2);
                assert!(task.take_ready());
                drop(task);
                io.wake();
            }),
            0
        );
    }

    #[cfg(feature = "std")]
    #[test]
    fn concurrent_completions_keep_one_owner_until_all_wakers_retire() {
        let task = TaskWake::new();
        let parent = Arc::new(Parent(AtomicUsize::new(0)));
        let host: Waker = parent.clone().into();
        assert!(task.take_ready());
        let io = task.register(&host).clone();
        std::thread::scope(|scope| {
            for _ in 0..4 {
                let io = io.clone();
                scope.spawn(move || {
                    for _ in 0..1000 {
                        io.clone().wake();
                    }
                });
            }
        });
        assert!(task.take_ready());
        assert_eq!(parent.0.load(Ordering::Relaxed), 1);
        drop(task);
        io.wake();
    }
}
