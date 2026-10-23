//! Resource readiness shared with I/O completion threads and interrupt handlers.
//! Notification never polls a loader or touches reactive state.

use alloc::boxed::Box;
use core::{
    cell::RefCell,
    task::{RawWaker, RawWakerVTable, Waker},
};

struct WakeData<const N: usize> {
    owners: usize,
    ready: bool,
    parents: [Option<Waker>; N],
}

struct WakeState<const N: usize>(critical_section::Mutex<RefCell<WakeData<N>>>);

pub(crate) type TaskWake = WakeSet<1>;

pub(crate) struct WakeSet<const N: usize> {
    waker: Waker,
}

impl<const N: usize> WakeSet<N> {
    const VTABLE: RawWakerVTable =
        RawWakerVTable::new(clone::<N>, wake::<N>, wake_by_ref::<N>, release::<N>);

    pub fn new() -> Self {
        assert!(N > 0);
        let state = Box::new(WakeState::<N>(critical_section::Mutex::new(RefCell::new(
            WakeData {
                owners: 1,
                ready: true,
                parents: core::array::from_fn(|_| None),
            },
        ))));
        let raw = RawWaker::new(Box::into_raw(state).cast(), &Self::VTABLE);
        // SAFETY: the vtable owns the boxed state and synchronizes every reference-count and readiness access.
        Self {
            waker: unsafe { Waker::from_raw(raw) },
        }
    }

    fn state(&self) -> &WakeState<N> {
        // SAFETY: construction is private and this Waker retains one strong state reference.
        unsafe { &*self.waker.data().cast::<WakeState<N>>() }
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
            let old = core::mem::replace(&mut data.parents, core::array::from_fn(|_| None));
            data.parents[0] = Some(parent);
            let notify = data.ready.then(|| data.parents[0].take()).flatten();
            (old, notify)
        });
        // Arbitrary parent vtables run outside the critical section and outside every RefCell borrow.
        drop(old);
        if let Some(parent) = notify {
            parent.wake();
        }
        &self.waker
    }

    /// Register an additional consumer without replacing an earlier waiter or growing storage.
    pub fn register_shared(&self, parent: &Waker) -> Option<&Waker> {
        if parent.will_wake(&self.waker) {
            return Some(&self.waker);
        }
        let mut parent = Some(parent.clone());
        let (accepted, notify) = critical_section::with(|cs| {
            let mut data = self.state().0.borrow(cs).borrow_mut();
            if data.ready {
                return (true, parent.take());
            }
            if data
                .parents
                .iter()
                .flatten()
                .any(|old| old.will_wake(parent.as_ref().unwrap()))
            {
                return (true, None);
            }
            if let Some(slot) = data.parents.iter_mut().find(|slot| slot.is_none()) {
                *slot = parent.take();
                return (true, None);
            }
            (false, None)
        });
        if let Some(parent) = notify {
            parent.wake();
        }
        accepted.then_some(&self.waker)
    }

    pub fn schedule(&self) {
        self.waker.wake_by_ref();
    }

    pub fn waker(&self) -> &Waker {
        &self.waker
    }
}

unsafe fn clone<const N: usize>(data: *const ()) -> RawWaker {
    // SAFETY: every vtable invocation holds a live reference until it returns.
    let state = unsafe { &*data.cast::<WakeState<N>>() };
    critical_section::with(|cs| {
        let mut state = state.0.borrow(cs).borrow_mut();
        state.owners = state
            .owners
            .checked_add(1)
            .expect("resource waker reference overflow");
    });
    RawWaker::new(data, &WakeSet::<N>::VTABLE)
}

unsafe fn release<const N: usize>(data: *const ()) {
    // SAFETY: this invocation consumes exactly one counted reference.
    let state = unsafe { &*data.cast::<WakeState<N>>() };
    let last = critical_section::with(|cs| {
        let mut state = state.0.borrow(cs).borrow_mut();
        state.owners -= 1;
        state.owners == 0
    });
    if last {
        // SAFETY: the final reference exclusively owns the original Box allocation; parent destruction runs unlocked.
        drop(unsafe { Box::from_raw(data.cast_mut().cast::<WakeState<N>>()) });
    }
}

unsafe fn wake_by_ref<const N: usize>(data: *const ()) {
    // SAFETY: wake_by_ref borrows an existing counted reference.
    let state = unsafe { &*data.cast::<WakeState<N>>() };
    let parents = critical_section::with(|cs| {
        let mut state = state.0.borrow(cs).borrow_mut();
        state.ready = true;
        core::mem::replace(&mut state.parents, core::array::from_fn(|_| None))
    });
    for parent in parents.into_iter().flatten() {
        parent.wake();
    }
}

unsafe fn wake<const N: usize>(data: *const ()) {
    struct Release<const N: usize>(*const ());
    impl<const N: usize> Drop for Release<N> {
        fn drop(&mut self) {
            // SAFETY: this guard owns the reference consumed by wake, including during unwinding.
            unsafe { release::<N>(self.0) };
        }
    }
    let _release = Release::<N>(data);
    // SAFETY: the guard retains the reference throughout notification.
    unsafe { wake_by_ref::<N>(data) };
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

    #[test]
    fn shared_waiters_deduplicate_reject_overflow_and_survive_owner_drop() {
        let task = WakeSet::<2>::new();
        let parents: [_; 3] = core::array::from_fn(|_| Arc::new(Parent(AtomicUsize::new(0))));
        let wakers = parents.each_ref().map(|parent| Waker::from(parent.clone()));
        assert!(task.take_ready());
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                for _ in 0..3 {
                    assert!(task.register_shared(&wakers[0]).is_some());
                    assert!(task.register_shared(&wakers[1]).is_some());
                    assert!(task.register_shared(&wakers[2]).is_none());
                    task.schedule();
                    assert!(task.take_ready());
                }
                assert!(task.register_shared(&wakers[0]).is_some());
                assert!(task.register_shared(&wakers[1]).is_some());
                let io = task.waker().clone();
                drop(task);
                io.wake();
            }),
            0
        );
        assert_eq!(parents[0].0.load(Ordering::Relaxed), 4);
        assert_eq!(parents[1].0.load(Ordering::Relaxed), 4);
        assert_eq!(parents[2].0.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn shared_registration_after_completion_notifies_each_late_consumer() {
        let task = WakeSet::<2>::new();
        let parent = Arc::new(Parent(AtomicUsize::new(0)));
        let host: Waker = parent.clone().into();
        assert!(task.register_shared(&host).is_some());
        assert_eq!(parent.0.load(Ordering::Relaxed), 1);
        assert!(task.take_ready());
        assert!(task.register_shared(&host).is_some());
        task.schedule();
        assert_eq!(parent.0.load(Ordering::Relaxed), 2);
        assert!(task.register_shared(&host).is_some());
        assert_eq!(parent.0.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn shared_notification_allows_reentrant_registration() {
        struct Reentrant(WakeSet<2>, Waker);
        impl Wake for Reentrant {
            fn wake(self: Arc<Self>) {
                self.0.take_ready();
                assert!(self.0.register_shared(&self.1).is_some());
            }
        }
        let parent = Arc::new(Parent(AtomicUsize::new(0)));
        let shared = Arc::new(Reentrant(WakeSet::new(), parent.clone().into()));
        let host: Waker = shared.clone().into();
        shared.0.take_ready();
        shared.0.register_shared(&host).unwrap();
        shared.0.schedule();
        shared.0.schedule();
        assert_eq!(parent.0.load(Ordering::Relaxed), 1);
    }
}
