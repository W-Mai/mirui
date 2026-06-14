extern crate alloc;

use alloc::collections::{BTreeMap, VecDeque};
use alloc::rc::{Rc, Weak};
use alloc::vec::Vec;
use core::cell::RefCell;

use crate::ecs::{Entity, World};
use crate::widget::dirty::Dirty;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Subscriber {
    Widget(Entity),
    Effect(EffectId),
    Computed(ComputedId),
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct EffectId(usize);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct ComputedId(usize);

struct Reactive {
    scope: Option<Subscriber>,
    // non-null only while an effect runs; lets a Fn() closure reach the World
    world: *mut World,
    dirty_widgets: VecDeque<Entity>,
    dirty_effects: VecDeque<EffectId>,
    effects: BTreeMap<EffectId, Rc<RefCell<EffectInner>>>,
    computeds: BTreeMap<ComputedId, Weak<dyn ComputedNode>>,
}

impl Reactive {
    const fn new() -> Self {
        Reactive {
            scope: None,
            world: core::ptr::null_mut(),
            dirty_widgets: VecDeque::new(),
            dirty_effects: VecDeque::new(),
            effects: BTreeMap::new(),
            computeds: BTreeMap::new(),
        }
    }
}

// Ambient runtime: `Signal::get()` is parameterless, so it finds the current
// consumer here rather than via a passed context. Storage is sealed in this fn
// — std uses a per-thread cell (tests isolate, no lock), no_std a critical_section.
#[cfg(feature = "std")]
std::thread_local! {
    static RT: RefCell<Reactive> = const { RefCell::new(Reactive::new()) };
}

fn with_reactive<R>(f: impl FnOnce(&mut Reactive) -> R) -> R {
    #[cfg(feature = "std")]
    {
        RT.with(|rt| f(&mut rt.borrow_mut()))
    }
    #[cfg(not(feature = "std"))]
    {
        // Single-core: critical_section serializes access, so the &mut is
        // unique for the section. static mut (vs Mutex<RefCell>) carries no
        // Send/Sync bound, so the Rc-handle effect registry can live here.
        // Same pattern as perf::with_state.
        static mut RT: Reactive = Reactive::new();
        critical_section::with(|_| {
            #[allow(static_mut_refs)]
            unsafe {
                f(&mut RT)
            }
        })
    }
}

// Drop impls run during thread-local destruction (a leaked effect holding a
// Signal/Computed gets dropped when the runtime cell tears down), when `RT` is
// no longer accessible. Use this from Drop so teardown silently no-ops.
fn try_with_reactive<R>(f: impl FnOnce(&mut Reactive) -> R) -> Option<R> {
    #[cfg(feature = "std")]
    {
        RT.try_with(|rt| f(&mut rt.borrow_mut())).ok()
    }
    #[cfg(not(feature = "std"))]
    {
        Some(with_reactive(f))
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

fn enqueue_effect(id: EffectId) {
    with_reactive(|r| r.dirty_effects.push_back(id));
}

struct WorldGuard {
    prev: *mut World,
}

impl WorldGuard {
    fn enter(world: &mut World) -> Self {
        let prev = with_reactive(|r| core::mem::replace(&mut r.world, world as *mut World));
        WorldGuard { prev }
    }
}

impl Drop for WorldGuard {
    fn drop(&mut self) {
        let prev = self.prev;
        with_reactive(|r| r.world = prev);
    }
}

/// Make the World reachable by effect closures while `f` runs — applies a
/// reactive binding's initial value at `ui!` construction, outside the flush.
#[cfg_attr(not(test), allow(dead_code))]
pub fn with_world_scope<R>(world: &mut World, f: impl FnOnce() -> R) -> R {
    let _guard = WorldGuard::enter(world);
    f()
}

/// Reach the World the running effect is under; None outside a flush window.
/// Public so `ui!`-generated reactive control flow can reach it from user crates.
pub fn with_world<R>(f: impl FnOnce(&mut World) -> R) -> Option<R> {
    // Copy the pointer out before deref so `f` may re-enter with_reactive.
    let ptr = with_reactive(|r| r.world);
    if ptr.is_null() {
        return None;
    }
    // SAFETY: non-null only within flush_signal_dirty / with_world_scope, which
    // hold a live &mut World; single-threaded. Windows may nest (a reactive
    // walk/if/match re-run enters another scope); WorldGuard saves/restores the
    // previous pointer LIFO, so the innermost live &mut World always wins.
    Some(f(unsafe { &mut *ptr }))
}

// An effect re-running may set signals that enqueue more effects/widgets in the
// same frame; loop until both queues settle. The cap stops a runaway cycle from
// hanging the frame — real cycle detection lands later.
const FLUSH_MAX_PASSES: u32 = 32;

/// Drain queued reactive work once per frame, after systems and before render:
/// re-run dirty effects (which may dirty more), then mark dirty widgets. Dead
/// entities are skipped (no reverse index; subscriber lists may retain
/// despawned entities).
pub fn flush_signal_dirty(world: &mut World) {
    let _guard = WorldGuard::enter(world);
    reclaim_dead_effects(world);
    for _ in 0..FLUSH_MAX_PASSES {
        let effects: Vec<EffectId> = with_reactive(|r| r.dirty_effects.drain(..).collect());
        for id in &effects {
            run_effect(*id);
        }
        let widgets: Vec<Entity> = with_reactive(|r| r.dirty_widgets.drain(..).collect());
        for entity in &widgets {
            if world.is_alive(*entity) {
                world.insert(*entity, Dirty);
            }
        }
        let settled = with_reactive(|r| r.dirty_effects.is_empty() && r.dirty_widgets.is_empty());
        if effects.is_empty() && widgets.is_empty() && settled {
            return;
        }
    }
    // cap reached = a cycle; debug panics, release stops instead of hanging
    debug_assert!(
        false,
        "reactive flush did not settle in {FLUSH_MAX_PASSES} passes (cycle?)"
    );
}

// Release cycle warning, wired in once a no_std log facade exists.
#[cfg_attr(not(test), allow(dead_code))]
fn warn_cycle_once() {
    use core::sync::atomic::{AtomicBool, Ordering};
    static WARNED: AtomicBool = AtomicBool::new(false);
    if !WARNED.swap(true, Ordering::Relaxed) {
        // TODO(log): "reactive flush hit the pass cap; cycle suspected"
    }
}

// Reclaim effects whose owning widget is gone. No ECS hook — flush holds the
// World, same lazy-liveness approach as the dirty-widget skip.
fn reclaim_dead_effects(world: &World) {
    let dead: Vec<EffectId> = with_reactive(|r| {
        r.effects
            .iter()
            .filter(|(_, e)| e.borrow().owner_entity.is_some_and(|o| !world.is_alive(o)))
            .map(|(id, _)| *id)
            .collect()
    });
    for id in dead {
        with_reactive(|r| r.effects.remove(&id));
    }
    with_reactive(|r| r.computeds.retain(|_, w| w.strong_count() > 0));
}

/// Reclaim every effect bound to `entity` now, instead of waiting for the next
/// flush. For a widget-teardown path to call when it truly despawns a subtree.
#[cfg_attr(not(test), allow(dead_code))]
pub fn cleanup_effects_for(entity: Entity) {
    let bound: Vec<EffectId> = with_reactive(|r| {
        r.effects
            .iter()
            .filter(|(_, e)| e.borrow().owner_entity == Some(entity))
            .map(|(id, _)| *id)
            .collect()
    });
    for id in bound {
        with_reactive(|r| r.effects.remove(&id));
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
            propagate(sub);
        }
    }
}

fn propagate(sub: Subscriber) {
    match sub {
        Subscriber::Widget(entity) => enqueue_widget(entity),
        Subscriber::Effect(id) => enqueue_effect(id),
        Subscriber::Computed(id) => mark_computed_dirty(id),
    }
}

// A source changed, so this computed's cache is stale: flag it and propagate to
// its own subscribers. Pure data mutation (no recompute, no closure) — the
// actual recompute is lazy, deferred to the next get(). Pulls subscribers out
// before recursing so a nested computed chain can't hold a borrow across calls.
fn mark_computed_dirty(id: ComputedId) {
    let node = with_reactive(|r| r.computeds.get(&id).and_then(Weak::upgrade));
    let Some(node) = node else {
        try_with_reactive(|r| r.computeds.remove(&id));
        return;
    };
    let already_dirty = node.mark_dirty_take_was_dirty();
    if already_dirty {
        return;
    }
    for sub in node.subscribers() {
        propagate(sub);
    }
}

struct EffectInner {
    run: Rc<dyn Fn()>,
    owner_entity: Option<Entity>,
}

/// A reactive side effect. Runs its closure once on creation to subscribe to
/// the signals it reads, then re-runs whenever any of them changes. Drop or
/// [`Effect::dispose`] to stop and unregister it.
pub struct Effect {
    id: EffectId,
}

impl Effect {
    pub fn new(f: impl Fn() + 'static) -> Effect {
        Self::spawn(f, None)
    }

    /// The widget this effect is bound to, if created via [`effect_with_widget`].
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn owner(&self) -> Option<Entity> {
        with_reactive(|r| {
            r.effects
                .get(&self.id)
                .and_then(|e| e.borrow().owner_entity)
        })
    }

    fn spawn(f: impl Fn() + 'static, owner_entity: Option<Entity>) -> Effect {
        let inner = Rc::new(RefCell::new(EffectInner {
            run: Rc::new(f),
            owner_entity,
        }));
        let id = EffectId(Rc::as_ptr(&inner) as *const () as usize);
        with_reactive(|r| {
            r.effects.insert(id, inner);
        });
        run_effect(id);
        Effect { id }
    }

    /// Stop and unregister now (same as dropping the handle); for standalone
    /// effects — widget-bound ones are reclaimed when their widget despawns.
    pub fn dispose(self) {}
}

impl Drop for Effect {
    fn drop(&mut self) {
        try_with_reactive(|r| {
            r.effects.remove(&self.id);
        });
    }
}

/// An effect owned by the runtime and tied to a widget entity: it re-runs on
/// dependency change and lives until the widget is despawned (cleanup later),
/// not until a returned handle drops. Hence no handle is returned — dropping
/// one would immediately unregister the effect.
#[cfg_attr(not(test), allow(dead_code))]
pub fn effect_with_widget(entity: Entity, f: impl Fn() + 'static) {
    core::mem::forget(Effect::spawn(f, Some(entity)));
}

// Clone the closure Rc out under the lock, then run it OUTSIDE — running user
// code while holding the no_std critical_section would re-enter and deadlock.
fn run_effect(id: EffectId) {
    let run = with_reactive(|r| r.effects.get(&id).map(|e| Rc::clone(&e.borrow().run)));
    if let Some(run) = run {
        with_scope(Subscriber::Effect(id), || run());
    }
}

// Type-erased view of a Computed so the runtime registry can hold mixed `T`
// without a generic. Only the non-generic propagation hooks are exposed.
trait ComputedNode {
    fn mark_dirty_take_was_dirty(&self) -> bool;
    fn subscribers(&self) -> Vec<Subscriber>;
}

struct ComputedInner<T> {
    value: Option<T>,
    compute: alloc::boxed::Box<dyn Fn() -> T>,
    subscribers: Vec<Subscriber>,
    dirty: bool,
}

impl<T> ComputedNode for RefCell<ComputedInner<T>> {
    fn mark_dirty_take_was_dirty(&self) -> bool {
        let was = self.borrow().dirty;
        self.borrow_mut().dirty = true;
        was
    }

    fn subscribers(&self) -> Vec<Subscriber> {
        self.borrow().subscribers.clone()
    }
}

/// A lazily-recomputed derived value. Reads its sources through `get()`, so it
/// subscribes to them; when a source changes the cached value is invalidated
/// and recomputed on the next `get()`. Cheap to clone (shared handle).
pub struct Computed<T: 'static> {
    inner: Rc<RefCell<ComputedInner<T>>>,
}

impl<T: 'static> Clone for Computed<T> {
    fn clone(&self) -> Self {
        Computed {
            inner: Rc::clone(&self.inner),
        }
    }
}

impl<T: 'static> Computed<T> {
    pub fn new(f: impl Fn() -> T + 'static) -> Self {
        let inner = Rc::new(RefCell::new(ComputedInner {
            value: None,
            compute: alloc::boxed::Box::new(f),
            subscribers: Vec::new(),
            dirty: true,
        }));
        let id = ComputedId(Rc::as_ptr(&inner) as *const () as usize);
        let node: Rc<dyn ComputedNode> = inner.clone();
        with_reactive(|r| {
            r.computeds.insert(id, Rc::downgrade(&node));
        });
        Computed { inner }
    }

    fn id(&self) -> ComputedId {
        ComputedId(Rc::as_ptr(&self.inner) as *const () as usize)
    }

    pub fn get(&self) -> T
    where
        T: Clone,
    {
        let id = self.id();
        if let Some(sub) = current_scope() {
            let mut inner = self.inner.borrow_mut();
            if !inner.subscribers.contains(&sub) {
                inner.subscribers.push(sub);
            }
        }
        if self.inner.borrow().dirty {
            // Recompute in this computed's scope so its sources subscribe IT,
            // not whatever outer consumer triggered the read.
            let value = with_scope(Subscriber::Computed(id), || (self.inner.borrow().compute)());
            let mut inner = self.inner.borrow_mut();
            inner.value = Some(value);
            inner.dirty = false;
        }
        self.inner
            .borrow()
            .value
            .clone()
            .expect("computed value populated after recompute")
    }
}

// No Drop unregister: the registry holds a Weak, and clones share one
// allocation, so removing on any clone's drop would unregister a Computed
// other clones still use. The Weak dangles once the last clone drops; stale
// entries are skipped on upgrade (and cleared opportunistically in
// mark_computed_dirty).

#[cfg(test)]
mod tests {
    use super::*;

    // std test threads are pooled and reused, so the per-thread runtime can
    // carry residue between tests on the same thread — reset at entry.
    fn reset() {
        with_reactive(|r| {
            r.scope = None;
            r.dirty_widgets.clear();
            r.dirty_effects.clear();
            r.effects.clear();
            r.computeds.clear();
        });
    }

    fn entity(id: u32) -> Entity {
        Entity { id, generation: 0 }
    }

    fn drain_effects() {
        loop {
            let batch: Vec<EffectId> = with_reactive(|r| r.dirty_effects.drain(..).collect());
            if batch.is_empty() {
                break;
            }
            for id in batch {
                run_effect(id);
            }
        }
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

    #[test]
    fn effect_runs_once_on_creation() {
        reset();
        let runs = Rc::new(RefCell::new(0));
        let r = Rc::clone(&runs);
        let _e = Effect::new(move || *r.borrow_mut() += 1);
        assert_eq!(*runs.borrow(), 1);
    }

    #[test]
    fn effect_reruns_on_dependency_change() {
        reset();
        let s = Signal::new(0i32);
        let seen = Rc::new(RefCell::new(alloc::vec::Vec::<i32>::new()));
        let (sc, seenc) = (s.clone(), Rc::clone(&seen));
        let _e = Effect::new(move || seenc.borrow_mut().push(sc.get()));
        assert_eq!(*seen.borrow(), alloc::vec![0]);

        s.set(7);
        drain_effects();
        assert_eq!(*seen.borrow(), alloc::vec![0, 7]);
    }

    #[test]
    fn disposed_effect_stops_rerunning() {
        reset();
        let s = Signal::new(0i32);
        let runs = Rc::new(RefCell::new(0));
        let (sc, rc) = (s.clone(), Rc::clone(&runs));
        let e = Effect::new(move || {
            let _ = sc.get();
            *rc.borrow_mut() += 1;
        });
        assert_eq!(*runs.borrow(), 1);
        e.dispose();
        s.set(1);
        drain_effects();
        assert_eq!(*runs.borrow(), 1, "disposed effect must not re-run");
    }

    #[test]
    fn spawn_records_owner_entity() {
        reset();
        let w = entity(99);
        let owned = Effect::spawn(|| {}, Some(w));
        assert_eq!(owned.owner(), Some(w));
        let standalone = Effect::new(|| {});
        assert_eq!(standalone.owner(), None);
    }

    #[test]
    fn effect_setting_signal_during_flush_does_not_deadlock() {
        reset();
        let trigger = Signal::new(0i32);
        let target = Signal::new(0i32);
        let (tc, gc) = (trigger.clone(), target.clone());
        let _e = Effect::new(move || {
            let v = tc.get();
            if v > 0 {
                gc.set(v * 2);
            }
        });
        trigger.set(5);
        drain_effects();
        assert_eq!(target.get_untracked(), 10);
    }

    #[test]
    fn computed_derives_and_recomputes() {
        reset();
        let n = Signal::new(2i32);
        let nc = n.clone();
        let doubled = Computed::new(move || nc.get() * 2);
        assert_eq!(doubled.get(), 4);
        n.set(5);
        assert_eq!(doubled.get(), 10);
    }

    #[test]
    fn computed_is_lazy_until_get() {
        reset();
        let n = Signal::new(1i32);
        let calls = Rc::new(RefCell::new(0));
        let (nc, cc) = (n.clone(), Rc::clone(&calls));
        let c = Computed::new(move || {
            *cc.borrow_mut() += 1;
            nc.get()
        });
        assert_eq!(*calls.borrow(), 0, "no compute before first get");
        let _ = c.get();
        assert_eq!(*calls.borrow(), 1);
        let _ = c.get();
        assert_eq!(*calls.borrow(), 1, "clean re-get does not recompute");
        n.set(2);
        let _ = c.get();
        assert_eq!(*calls.borrow(), 2, "recompute only after a source change");
    }

    #[test]
    fn computed_source_change_dirties_subscribing_widget() {
        reset();
        let n = Signal::new(0i32);
        let nc = n.clone();
        let c = Computed::new(move || nc.get() + 1);
        let w = entity(7);
        with_scope(Subscriber::Widget(w), || {
            let _ = c.get();
        });
        n.set(9);
        with_reactive(|r| {
            assert!(
                r.dirty_widgets.contains(&w),
                "source change cascades to widget via computed"
            );
            r.dirty_widgets.clear();
        });
    }

    #[test]
    fn chained_computeds_propagate() {
        reset();
        let n = Signal::new(1i32);
        let nc = n.clone();
        let a = Computed::new(move || nc.get() + 1);
        let ac = a.clone();
        let b = Computed::new(move || ac.get() * 10);
        assert_eq!(b.get(), 20);
        n.set(4);
        assert_eq!(b.get(), 50, "change flows source -> a -> b");
    }

    fn effect_count() -> usize {
        with_reactive(|r| r.effects.len())
    }

    #[test]
    fn despawned_widget_effect_is_reclaimed_on_flush() {
        reset();
        let mut world = World::new();
        let e = world.spawn_empty();
        let s = Signal::new(0i32);
        let sc = s.clone();
        with_world_scope(&mut world, || {
            effect_with_widget(e, move || {
                let _ = sc.get();
            })
        });
        assert_eq!(effect_count(), 1);

        world.despawn(e);
        // quiet effect: its signal never fires again, only the sweep reclaims it
        flush_signal_dirty(&mut world);
        assert_eq!(effect_count(), 0, "sweep drops the dead-owner effect");
    }

    #[test]
    fn live_widget_effect_survives_flush() {
        reset();
        let mut world = World::new();
        let e = world.spawn_empty();
        let s = Signal::new(0i32);
        let sc = s.clone();
        with_world_scope(&mut world, || {
            effect_with_widget(e, move || {
                let _ = sc.get();
            })
        });
        flush_signal_dirty(&mut world);
        assert_eq!(effect_count(), 1, "live owner keeps its effect");
    }

    #[test]
    fn dispose_unregisters_standalone_effect() {
        reset();
        let eff = Effect::new(|| {});
        assert_eq!(effect_count(), 1);
        eff.dispose();
        assert_eq!(effect_count(), 0);
    }

    #[test]
    fn self_feeding_effect_terminates_within_cap() {
        reset();
        let mut world = World::new();
        let s = Signal::new(0i32);
        let sc = s.clone();
        core::mem::forget(Effect::new(move || {
            let v = sc.get();
            if v < 5 {
                sc.set(v + 1);
            }
        }));
        s.set(1);
        flush_signal_dirty(&mut world); // must return, not hang
    }

    #[test]
    fn nested_world_scope_restores_outer_pointer() {
        reset();
        let mut outer = World::new();
        with_world_scope(&mut outer, || {
            let mut inner = World::new();
            with_world_scope(&mut inner, || {
                assert!(with_world(|_| ()).is_some(), "inner scope sees a world");
            });
            assert!(
                with_world(|_| ()).is_some(),
                "outer scope still reachable after inner drops",
            );
        });
        assert!(with_world(|_| ()).is_none(), "no world outside any scope");
    }
}
