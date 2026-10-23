use super::*;
use crate::core::cache::{HasSize, MaxSize};
use alloc::{rc::Rc, vec::Vec};
use core::{
    cell::{Cell, RefCell},
    task::{Context, Poll, Waker},
};

#[derive(Debug, PartialEq)]
struct Value(u32);
impl HasSize for Value {
    fn cache_size(&self) -> usize {
        4
    }
}
impl HasProbe for Value {
    type Meta = Value;
    fn extract_meta(&self) -> Self::Meta {
        Value(self.0)
    }
}
impl Clone for Value {
    fn clone(&self) -> Self {
        Value(self.0)
    }
}

#[derive(Default)]
struct Control {
    ready: Cell<bool>,
    calls: Cell<usize>,
    probes: Cell<usize>,
    resets: Cell<usize>,
    wakers: RefCell<Vec<Waker>>,
}
impl Control {
    fn complete(&self) {
        self.ready.set(true);
        for wake in self.wakers.borrow().iter() {
            wake.wake_by_ref();
        }
    }
}

struct Decoder(Rc<Control>);
impl Loader<Value> for Decoder {
    type State = u32;
    fn recognize(&self, _: LoadInput<'_>) -> FormatMatch {
        FormatMatch::Match
    }
    fn poll_load(
        &self,
        state: &mut u32,
        cx: &mut LoadContext<'_>,
    ) -> Poll<Result<Value, LoadError>> {
        self.0.calls.set(self.0.calls.get() + 1);
        if !self.0.ready.get() {
            if *state == 0 {
                self.0.wakers.borrow_mut().push(cx.waker().clone());
                *state = 1;
            }
            return Poll::Pending;
        }
        Poll::Ready(Ok(Value(*state + 40)))
    }
    fn reset(&self, state: &mut u32) {
        *state = 0;
        self.0.resets.set(self.0.resets.get() + 1);
    }
}
impl ProbeLoader<Value> for Decoder {
    fn poll_probe(
        &self,
        state: &mut u32,
        cx: &mut LoadContext<'_>,
    ) -> Poll<Result<Value, LoadError>> {
        self.0.probes.set(self.0.probes.get() + 1);
        if *state == 0 {
            self.0.wakers.borrow_mut().push(cx.waker().clone());
            *state = 2;
        }
        if !self.0.ready.get() {
            return Poll::Pending;
        }
        Poll::Ready(Ok(Value(7)))
    }
}

fn tick(manager: &ResourceManager<Value>) {
    manager.poll_pending(&mut Context::from_waker(Waker::noop()));
}

#[test]
fn pending_tasks_sleep_until_woken_then_publish_to_existing_handles() {
    let manager = ResourceManager::new(MaxSize::Count(8), Value(0));
    let control = Rc::new(Control::default());
    manager.add_loader(Decoder(control.clone()));
    let handle = manager.load("value");
    assert_eq!(handle.get().0, 0);
    assert_eq!(control.calls.get(), 1);
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            for _ in 0..10 {
                tick(&manager);
                assert_eq!(handle.get().0, 0);
            }
        }),
        0
    );
    assert_eq!(control.calls.get(), 1);
    control.complete();
    tick(&manager);
    assert_eq!(handle.get().0, 41);
    assert_eq!(control.calls.get(), 2);
    assert_eq!(control.resets.get(), 1);
}

#[test]
fn probe_then_load_keeps_decoder_state_and_does_not_decode_for_metadata() {
    for request_during_probe in [false, true] {
        let manager = ResourceManager::new(MaxSize::Count(8), Value(0));
        let control = Rc::new(Control::default());
        manager.add_probed_loader(Decoder(control.clone()));
        assert_eq!(manager.probe("value"), None);
        if request_during_probe {
            assert_eq!(manager.resolve("value").0, 0);
        }
        tick(&manager);
        assert_eq!(control.probes.get(), 1);
        assert_eq!(control.calls.get(), 0);
        control.complete();
        tick(&manager);
        if request_during_probe {
            tick(&manager);
        } else {
            assert_eq!(manager.probe("value"), Some(Value(7)));
            assert_eq!(control.calls.get(), 0);
        }
        assert_eq!(manager.resolve("value").0, 42);
        assert_eq!(control.resets.get(), 1);
    }
}

#[test]
fn replacement_rejects_late_values_and_returns_task_capacity() {
    let manager = ResourceManager::new(MaxSize::Count(8), Value(0));
    let control = Rc::new(Control::default());
    manager.add_loader(Decoder(control.clone()));
    for token in ["a", "b", "c", "d"] {
        assert_eq!(manager.resolve(token).0, 0);
    }
    assert_eq!(control.calls.get(), 4);
    assert_eq!(manager.resolve("full").0, 0);
    assert_eq!(control.calls.get(), 4);
    manager.add_static("a", Value(100));
    manager.remove_token("b");
    tick(&manager);
    assert_eq!(control.resets.get(), 2);
    control.complete();
    tick(&manager);
    assert_eq!(manager.resolve("a").0, 100);
    assert_eq!(manager.resolve("c").0, 41);
    manager.clear_failed("full");
    assert_eq!(manager.resolve("full").0, 40);
    assert_eq!(control.resets.get(), 5);
}

#[test]
fn asynchronous_probe_failure_is_not_restarted_by_layout_queries() {
    struct Failing(Rc<Control>);
    impl Loader<Value> for Failing {
        type State = ();
        fn recognize(&self, _: LoadInput<'_>) -> FormatMatch {
            FormatMatch::Match
        }
        fn poll_load(&self, _: &mut (), _: &mut LoadContext<'_>) -> Poll<Result<Value, LoadError>> {
            Poll::Ready(Ok(Value(99)))
        }
    }
    impl ProbeLoader<Value> for Failing {
        fn poll_probe(
            &self,
            _: &mut (),
            cx: &mut LoadContext<'_>,
        ) -> Poll<Result<Value, LoadError>> {
            self.0.probes.set(self.0.probes.get() + 1);
            if !self.0.ready.get() {
                self.0.wakers.borrow_mut().push(cx.waker().clone());
                return Poll::Pending;
            }
            Poll::Ready(Err(LoadError::Failed("invalid header")))
        }
    }
    let control = Rc::new(Control::default());
    let manager = ResourceManager::new(MaxSize::Count(8), Value(0));
    manager.add_probed_loader(Failing(control.clone()));
    assert_eq!(manager.probe("value"), None);
    control.complete();
    tick(&manager);
    for _ in 0..10 {
        assert_eq!(manager.probe("value"), None);
    }
    assert_eq!(control.probes.get(), 2);
    assert_eq!(manager.resolve("value").0, 99);
}

#[test]
fn delayed_not_mine_continues_the_same_chain_without_repeating_earlier_loaders() {
    struct Decline(Rc<Control>);
    impl Loader<Value> for Decline {
        type State = ();
        fn recognize(&self, _: LoadInput<'_>) -> FormatMatch {
            FormatMatch::Match
        }
        fn poll_load(
            &self,
            _: &mut (),
            cx: &mut LoadContext<'_>,
        ) -> Poll<Result<Value, LoadError>> {
            self.0.calls.set(self.0.calls.get() + 1);
            if !self.0.ready.get() {
                self.0.wakers.borrow_mut().push(cx.waker().clone());
                return Poll::Pending;
            }
            Poll::Ready(Err(LoadError::NotMine))
        }
    }
    let control = Rc::new(Control::default());
    let manager = ResourceManager::new(MaxSize::Count(8), Value(0));
    manager.add_loader(Decline(control.clone()));
    manager.add_loader(|_: &str| Ok(Value(77)));
    assert_eq!(manager.resolve("value").0, 0);
    control.complete();
    tick(&manager);
    assert_eq!(manager.resolve("value").0, 77);
    assert_eq!(control.calls.get(), 2);
}

#[cfg(feature = "std")]
#[test]
fn unwinding_poll_and_reset_restore_prepared_slots_and_request_guards() {
    struct Panics(Rc<Cell<u8>>);
    impl Loader<Value> for Panics {
        type State = ();
        fn recognize(&self, _: LoadInput<'_>) -> FormatMatch {
            FormatMatch::Match
        }
        fn poll_load(
            &self,
            _: &mut (),
            cx: &mut LoadContext<'_>,
        ) -> Poll<Result<Value, LoadError>> {
            match self.0.replace(self.0.get() + 1) {
                0 => {
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
                1 => panic!("poll"),
                _ => Poll::Ready(Ok(Value(8))),
            }
        }
        fn reset(&self, _: &mut ()) {
            if self.0.get() == 3 {
                panic!("reset");
            }
        }
    }
    let manager = ResourceManager::new(MaxSize::Count(8), Value(0));
    manager.add_loader(Panics(Rc::new(Cell::new(0))));
    assert_eq!(manager.resolve("value").0, 0);
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| tick(&manager))).is_err());
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| manager.resolve("value")))
            .is_err()
    );
    assert_eq!(manager.resolve("value").0, 8);
}
