extern crate alloc;

use alloc::collections::VecDeque;
use alloc::rc::Rc;
use alloc::vec::Vec;
use core::cell::RefCell;

use crate::ecs::{Entity, World};
use crate::widget::dirty::Dirty;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Subscriber {
    Widget(Entity),
}

struct Reactive {
    scope: Option<Subscriber>,
    dirty_widgets: VecDeque<Entity>,
}

impl Reactive {
    const fn new() -> Self {
        Reactive {
            scope: None,
            dirty_widgets: VecDeque::new(),
        }
    }
}

// Ambient runtime: `Signal::get()` is parameterless, so it finds the current
// consumer here rather than via a passed context. Storage is sealed in this fn
// — std uses a per-thread cell (tests isolate, no lock), no_std a critical_section.
fn with_reactive<R>(f: impl FnOnce(&mut Reactive) -> R) -> R {
    #[cfg(feature = "std")]
    {
        std::thread_local! {
            static RT: RefCell<Reactive> = const { RefCell::new(Reactive::new()) };
        }
        RT.with(|rt| f(&mut rt.borrow_mut()))
    }
    #[cfg(not(feature = "std"))]
    {
        static RT: critical_section::Mutex<RefCell<Reactive>> =
            critical_section::Mutex::new(RefCell::new(Reactive::new()));
        critical_section::with(|cs| f(&mut RT.borrow_ref_mut(cs)))
    }
}

fn current_scope() -> Option<Subscriber> {
    with_reactive(|r| r.scope)
}

/// Run `f` with `scope` as the active reactive consumer, restoring the
/// previous scope after. Signals read inside `f` subscribe to `scope`.
// Runtime callers (effects / inline-computed bindings) land in a later change.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn with_scope<R>(scope: Subscriber, f: impl FnOnce() -> R) -> R {
    let prev = with_reactive(|r| r.scope.replace(scope));
    let out = f();
    with_reactive(|r| r.scope = prev);
    out
}

fn enqueue_widget(entity: Entity) {
    with_reactive(|r| r.dirty_widgets.push_back(entity));
}

/// Drain queued signal-driven dirties into the world. Call once per frame
/// after systems, before render. Dead entities are skipped (no reverse
/// index; subscriber lists may retain despawned entities).
pub fn flush_signal_dirty(world: &mut World) {
    let drained: Vec<Entity> = with_reactive(|r| r.dirty_widgets.drain(..).collect());
    for entity in drained {
        if world.is_alive(entity) {
            world.insert(entity, Dirty);
        }
    }
}

struct SignalInner<T> {
    value: T,
    subscribers: Vec<Subscriber>,
}

pub struct Signal<T: 'static> {
    inner: Rc<RefCell<SignalInner<T>>>,
}

impl<T: 'static> Clone for Signal<T> {
    fn clone(&self) -> Self {
        Signal {
            inner: Rc::clone(&self.inner),
        }
    }
}

impl<T: 'static> Signal<T> {
    pub fn new(initial: T) -> Self {
        Signal {
            inner: Rc::new(RefCell::new(SignalInner {
                value: initial,
                subscribers: Vec::new(),
            })),
        }
    }

    fn track(&self) {
        if let Some(sub) = current_scope() {
            let mut inner = self.inner.borrow_mut();
            // dedup: an effect re-running re-reads the signal; without this the
            // subscriber accumulates duplicates and set() re-enqueues O(n) times.
            if !inner.subscribers.contains(&sub) {
                inner.subscribers.push(sub);
            }
        }
    }

    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.track();
        self.inner.borrow().value.clone()
    }

    pub fn get_untracked(&self) -> T
    where
        T: Clone,
    {
        self.inner.borrow().value.clone()
    }

    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        self.track();
        f(&self.inner.borrow().value)
    }

    pub fn set(&self, value: T) {
        {
            let mut inner = self.inner.borrow_mut();
            inner.value = value;
        }
        self.notify();
    }

    pub fn update(&self, f: impl FnOnce(&mut T)) {
        {
            let mut inner = self.inner.borrow_mut();
            f(&mut inner.value);
        }
        self.notify();
    }

    fn notify(&self) {
        let subs: Vec<Subscriber> = self.inner.borrow().subscribers.clone();
        for sub in subs {
            match sub {
                Subscriber::Widget(entity) => enqueue_widget(entity),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // std test threads are pooled and reused, so the per-thread runtime can
    // carry residue between tests on the same thread — reset at entry.
    fn reset() {
        with_reactive(|r| {
            r.scope = None;
            r.dirty_widgets.clear();
        });
    }

    fn entity(id: u32) -> Entity {
        Entity { id, generation: 0 }
    }

    #[test]
    fn get_set_update_roundtrip() {
        let s = Signal::new(1i32);
        assert_eq!(s.get(), 1);
        s.set(5);
        assert_eq!(s.get(), 5);
        s.update(|n| *n += 3);
        assert_eq!(s.get(), 8);
    }

    #[test]
    fn with_reads_without_clone() {
        let s = Signal::new(alloc::string::String::from("hi"));
        let len = s.with(|v| v.len());
        assert_eq!(len, 2);
    }

    #[test]
    fn clone_shares_state() {
        let a = Signal::new(0i32);
        let b = a.clone();
        a.set(42);
        assert_eq!(b.get(), 42);
    }

    #[test]
    fn read_in_scope_subscribes_and_set_enqueues() {
        reset();
        let s = Signal::new(0i32);
        let w = entity(1);
        with_scope(Subscriber::Widget(w), || {
            let _ = s.get();
        });
        s.set(1);
        with_reactive(|r| {
            assert_eq!(r.dirty_widgets.len(), 1);
            assert_eq!(r.dirty_widgets[0], w);
            r.dirty_widgets.clear();
        });
    }

    #[test]
    fn get_untracked_does_not_subscribe() {
        reset();
        let s = Signal::new(0i32);
        let w = entity(2);
        with_scope(Subscriber::Widget(w), || {
            let _ = s.get_untracked();
        });
        s.set(1);
        with_reactive(|r| {
            assert!(r.dirty_widgets.is_empty());
        });
    }

    #[test]
    fn repeated_reads_dedup_subscriber() {
        reset();
        let s = Signal::new(0i32);
        let w = entity(3);
        with_scope(Subscriber::Widget(w), || {
            let _ = s.get();
            let _ = s.get();
            let _ = s.get();
        });
        assert_eq!(s.inner.borrow().subscribers.len(), 1);
        s.set(1);
        with_reactive(|r| {
            assert_eq!(r.dirty_widgets.len(), 1);
            r.dirty_widgets.clear();
        });
    }

    #[test]
    fn read_outside_scope_no_subscribe() {
        reset();
        let s = Signal::new(0i32);
        let _ = s.get();
        s.set(1);
        with_reactive(|r| assert!(r.dirty_widgets.is_empty()));
    }

    #[test]
    fn nested_scope_restores_previous() {
        let outer = entity(10);
        let inner = entity(11);
        let s_outer = Signal::new(0i32);
        let s_inner = Signal::new(0i32);
        with_scope(Subscriber::Widget(outer), || {
            let _ = s_outer.get();
            with_scope(Subscriber::Widget(inner), || {
                let _ = s_inner.get();
            });
            let _ = s_outer.get();
        });
        assert_eq!(
            s_outer.inner.borrow().subscribers,
            alloc::vec![Subscriber::Widget(outer)]
        );
        assert_eq!(
            s_inner.inner.borrow().subscribers,
            alloc::vec![Subscriber::Widget(inner)]
        );
    }
}
