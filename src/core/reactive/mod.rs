extern crate alloc;

mod identity;

use alloc::collections::{BTreeMap, VecDeque};
use alloc::rc::{Rc, Weak};
use alloc::vec::Vec;
use core::cell::{Cell, RefCell};

use crate::ecs::world::WorldId;
use crate::ecs::{Entity, World};
use crate::ui::dirty::{Dirty, VisualDirty};
use identity::{SlotAllocator, SlotId};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Subscriber {
    Widget(Entity),
    VisualWidget(Entity),
    Effect(EffectId),
    Computed(ComputedId),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct OwnedSubscriber {
    world: Option<WorldId>,
    subscriber: Subscriber,
}

impl OwnedSubscriber {
    fn tracked(subscriber: Subscriber) -> Self {
        let world = match subscriber {
            Subscriber::Widget(_) | Subscriber::VisualWidget(_) => current_world_id(),
            Subscriber::Effect(_) | Subscriber::Computed(_) => None,
        };
        Self { world, subscriber }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct EffectId(SlotId);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct ComputedId(SlotId);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct SourceId(SlotId);

struct Reactive {
    scope: Option<Subscriber>,
    // non-null only while an effect runs; lets a Fn() closure reach the World
    world: *mut World,
    world_id: Option<WorldId>,
    borrowed_worlds: [Option<WorldId>; 8],
    model_read_only_depth: u16,
    dirty_widgets: VecDeque<(Option<WorldId>, Entity)>,
    dirty_visual_widgets: VecDeque<(Option<WorldId>, Entity)>,
    model_visual_subscriptions: usize,
    dirty_effects: VecDeque<EffectId>,
    effects: BTreeMap<EffectId, Rc<RefCell<EffectInner>>>,
    computeds: BTreeMap<ComputedId, Weak<dyn ComputedNode>>,
    effect_slots: SlotAllocator,
    computed_slots: SlotAllocator,
    source_slots: SlotAllocator,
}

impl Reactive {
    const fn new() -> Self {
        Reactive {
            scope: None,
            world: core::ptr::null_mut(),
            world_id: None,
            borrowed_worlds: [None; 8],
            model_read_only_depth: 0,
            dirty_widgets: VecDeque::new(),
            dirty_visual_widgets: VecDeque::new(),
            model_visual_subscriptions: 0,
            dirty_effects: VecDeque::new(),
            effects: BTreeMap::new(),
            computeds: BTreeMap::new(),
            effect_slots: SlotAllocator::new(),
            computed_slots: SlotAllocator::new(),
            source_slots: SlotAllocator::new(),
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

pub(crate) fn current_world_id() -> Option<WorldId> {
    with_reactive(|r| r.world_id)
}

pub(crate) fn model_writes_allowed() -> bool {
    with_reactive(|r| r.model_read_only_depth == 0)
}

pub(crate) struct ModelReadOnlyGuard;

impl ModelReadOnlyGuard {
    pub(crate) fn enter() -> Self {
        with_reactive(|r| {
            r.model_read_only_depth = r
                .model_read_only_depth
                .checked_add(1)
                .expect("model read-only scope overflow");
        });
        Self
    }
}

impl Drop for ModelReadOnlyGuard {
    fn drop(&mut self) {
        with_reactive(|r| r.model_read_only_depth -= 1);
    }
}

struct ScopeGuard(Option<Subscriber>);

impl Drop for ScopeGuard {
    fn drop(&mut self) {
        with_reactive(|r| r.scope = self.0);
    }
}

/// Run `f` with `scope` as the active reactive consumer, restoring the
/// previous scope after. Signals read inside `f` subscribe to `scope`.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn with_scope<R>(scope: Subscriber, f: impl FnOnce() -> R) -> R {
    let _guard = ScopeGuard(with_reactive(|r| r.scope.replace(scope)));
    f()
}

fn enqueue_widget(owner: Option<WorldId>, entity: Entity) {
    with_reactive(|r| r.dirty_widgets.push_back((owner, entity)));
}

fn enqueue_visual_widget(owner: Option<WorldId>, entity: Entity) {
    with_reactive(|r| {
        if !r.dirty_visual_widgets.contains(&(owner, entity)) {
            r.dirty_visual_widgets.push_back((owner, entity));
        }
    });
}

fn enqueue_effect(id: EffectId) {
    with_reactive(|r| r.dirty_effects.push_back(id));
}

pub(crate) struct OwnerGuard {
    prev: Option<WorldId>,
}

impl OwnerGuard {
    pub(crate) fn enter(world: &mut World) -> Self {
        let prev = with_reactive(|r| r.world_id.replace(world.id()));
        Self { prev }
    }
}

impl Drop for OwnerGuard {
    fn drop(&mut self) {
        with_reactive(|r| r.world_id = self.prev);
    }
}

pub(crate) struct WorldGuard {
    prev: *mut World,
    _owner: OwnerGuard,
}

impl WorldGuard {
    pub(crate) fn enter(world: &mut World) -> Self {
        let owner = OwnerGuard::enter(world);
        let prev = with_reactive(|r| core::mem::replace(&mut r.world, world as *mut World));
        Self {
            prev,
            _owner: owner,
        }
    }
}

struct WorldBorrowGuard {
    slot: usize,
}

impl WorldBorrowGuard {
    fn enter(id: WorldId) -> Self {
        let slot = with_reactive(|r| {
            assert!(
                !r.borrowed_worlds.contains(&Some(id)),
                "World is already mutably borrowed through with_world"
            );
            let slot = r
                .borrowed_worlds
                .iter()
                .position(Option::is_none)
                .expect("too many nested World borrows");
            r.borrowed_worlds[slot] = Some(id);
            slot
        });
        Self { slot }
    }
}

impl Drop for WorldBorrowGuard {
    fn drop(&mut self) {
        with_reactive(|r| r.borrowed_worlds[self.slot] = None);
    }
}

impl Drop for WorldGuard {
    fn drop(&mut self) {
        with_reactive(|r| r.world = self.prev);
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
    let (ptr, id) = with_reactive(|r| (r.world, r.world_id));
    if ptr.is_null() {
        return None;
    }
    let _borrow = WorldBorrowGuard::enter(id.expect("scoped World has an identity"));
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

fn drain_widgets_for(
    queue: &mut VecDeque<(Option<WorldId>, Entity)>,
    world_id: WorldId,
) -> Vec<Entity> {
    let mut ready = Vec::new();
    let pending = queue.len();
    for _ in 0..pending {
        let (owner, entity) = queue.pop_front().expect("queued widget");
        if owner.is_none_or(|owner| owner == world_id) {
            ready.push(entity);
        } else {
            queue.push_back((owner, entity));
        }
    }
    ready
}

/// Drain queued reactive work once per frame, after systems and before render:
/// re-run dirty effects (which may dirty more), then mark dirty widgets. Dead
/// entities are skipped (no reverse index; subscriber lists may retain
/// despawned entities).
pub fn flush_signal_dirty(world: &mut World) {
    let _guard = WorldGuard::enter(world);
    reclaim_dead_effects(world);
    let world_id = world.id();
    for _ in 0..FLUSH_MAX_PASSES {
        let effects: Vec<EffectId> = with_reactive(|r| {
            let mut ready = Vec::new();
            let pending = r.dirty_effects.len();
            for _ in 0..pending {
                let id = r.dirty_effects.pop_front().expect("queued effect");
                match r.effects.get(&id).map(|effect| effect.borrow().owner_world) {
                    Some(Some(owner)) if owner != world_id => r.dirty_effects.push_back(id),
                    Some(_) => ready.push(id),
                    None => {}
                }
            }
            ready
        });
        for id in &effects {
            run_effect(*id);
        }
        let widgets = with_reactive(|r| drain_widgets_for(&mut r.dirty_widgets, world_id));
        for entity in &widgets {
            if world.is_alive(*entity) {
                world.insert(*entity, Dirty);
            }
        }
        let visual_widgets =
            with_reactive(|r| drain_widgets_for(&mut r.dirty_visual_widgets, world_id));
        for entity in &visual_widgets {
            if world.is_alive(*entity) {
                world.insert(*entity, VisualDirty);
            }
        }
        if effects.is_empty() && widgets.is_empty() && visual_widgets.is_empty() {
            return;
        }
    }
    // cap reached = a cycle; debug panics, release stops instead of hanging
    debug_assert!(
        false,
        "reactive flush did not settle in {FLUSH_MAX_PASSES} passes (cycle?)"
    );
}

// Reclaim effects whose owning widget is gone. No ECS hook — flush holds the
// World, same lazy-liveness approach as the dirty-widget skip.
fn reclaim_dead_effects(world: &World) {
    let dead: Vec<EffectId> = with_reactive(|r| {
        r.effects
            .iter()
            .filter(|(_, e)| {
                let effect = e.borrow();
                effect.owner_world == Some(world.id())
                    && effect.owner_entity.is_some_and(|o| !world.is_alive(o))
            })
            .map(|(id, _)| *id)
            .collect()
    });
    for id in dead {
        unregister_effect(id);
    }
    with_reactive(|r| {
        let slots = &mut r.computed_slots;
        r.computeds.retain(|id, weak| {
            if weak.strong_count() > 0 {
                true
            } else {
                slots.release(id.0);
                false
            }
        });
    });
}

/// Reclaim effects bound to `entity` in `world` during widget teardown.
#[cfg_attr(not(test), allow(dead_code))]
pub fn cleanup_effects_for(world: &World, entity: Entity) {
    let bound: Vec<EffectId> = with_reactive(|r| {
        r.effects
            .iter()
            .filter(|(_, e)| {
                let effect = e.borrow();
                effect.owner_entity == Some(entity) && effect.owner_world == Some(world.id())
            })
            .map(|(id, _)| *id)
            .collect()
    });
    for id in bound {
        unregister_effect(id);
    }
}

struct SignalInner<T> {
    id: SourceId,
    value: T,
    subscribers: Vec<OwnedSubscriber>,
}

pub struct Signal<T: 'static> {
    inner: Rc<RefCell<SignalInner<T>>>,
}

pub(crate) struct SignalSubscription<T: 'static> {
    inner: Weak<RefCell<SignalInner<T>>>,
    subscriber: OwnedSubscriber,
}

impl<T: 'static> Drop for SignalSubscription<T> {
    fn drop(&mut self) {
        let Some(inner) = self.inner.upgrade() else {
            return;
        };
        inner
            .borrow_mut()
            .subscribers
            .retain(|subscriber| *subscriber != self.subscriber);
    }
}

impl<T: 'static> Clone for Signal<T> {
    fn clone(&self) -> Self {
        Signal {
            inner: Rc::clone(&self.inner),
        }
    }
}

impl<T: 'static> Drop for Signal<T> {
    fn drop(&mut self) {
        if Rc::strong_count(&self.inner) == 1 {
            let id = self.inner.borrow().id;
            try_with_reactive(|r| {
                r.source_slots.release(id.0);
            });
        }
    }
}

impl<T: 'static> Signal<T> {
    pub fn new(initial: T) -> Self {
        let id = with_reactive(|r| SourceId(r.source_slots.allocate()));
        Signal {
            inner: Rc::new(RefCell::new(SignalInner {
                id,
                value: initial,
                subscribers: Vec::new(),
            })),
        }
    }

    fn track(&self) {
        if let Some(sub) = current_scope() {
            self.add_subscriber(OwnedSubscriber::tracked(sub));
        }
    }

    fn add_subscriber(&self, subscriber: OwnedSubscriber) {
        let mut inner = self.inner.borrow_mut();
        if !inner.subscribers.contains(&subscriber) {
            inner.subscribers.push(subscriber);
        }
    }

    pub(crate) fn subscribe_widget(&self, world: WorldId, entity: Entity) -> SignalSubscription<T> {
        let subscriber = OwnedSubscriber {
            world: Some(world),
            subscriber: Subscriber::Widget(entity),
        };
        self.add_subscriber(subscriber);
        SignalSubscription {
            inner: Rc::downgrade(&self.inner),
            subscriber,
        }
    }

    pub(crate) fn subscribe_visual_widget(
        &self,
        world: WorldId,
        entity: Entity,
    ) -> SignalSubscription<T> {
        let subscriber = OwnedSubscriber {
            world: Some(world),
            subscriber: Subscriber::VisualWidget(entity),
        };
        self.add_subscriber(subscriber);
        SignalSubscription {
            inner: Rc::downgrade(&self.inner),
            subscriber,
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
        let id = self.inner.borrow().id;
        debug_assert!(with_reactive(|r| r.source_slots.is_live(id.0)));
        let subs: Vec<OwnedSubscriber> = self.inner.borrow().subscribers.clone();
        for sub in subs {
            propagate(sub);
        }
    }
}

fn propagate(sub: OwnedSubscriber) {
    match sub.subscriber {
        Subscriber::Widget(entity) => enqueue_widget(sub.world, entity),
        Subscriber::VisualWidget(entity) => enqueue_visual_widget(sub.world, entity),
        Subscriber::Effect(id) => enqueue_effect(id),
        Subscriber::Computed(id) => mark_computed_dirty(id),
    }
}

/// A source embedded in another owner rather than separately reference-counted.
#[doc(hidden)]
pub struct ModelSource {
    subscribers: RefCell<Vec<ModelSubscriber>>,
    next_subscription: Cell<u64>,
}

#[derive(Clone, Copy)]
struct ModelSubscriber {
    id: Option<u64>,
    subscriber: OwnedSubscriber,
}

impl Default for ModelSource {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelSource {
    pub const fn new() -> Self {
        Self {
            subscribers: RefCell::new(Vec::new()),
            next_subscription: Cell::new(1),
        }
    }

    pub fn track(&self) {
        let Some(subscriber) = current_scope() else {
            return;
        };
        let subscriber = OwnedSubscriber::tracked(subscriber);
        let mut subscribers = self.subscribers.borrow_mut();
        if !subscribers
            .iter()
            .any(|entry| entry.id.is_none() && entry.subscriber == subscriber)
        {
            subscribers.push(ModelSubscriber {
                id: None,
                subscriber,
            });
        }
    }

    pub(crate) fn subscribe_visual_widget(&self, world: WorldId, entity: Entity) -> u64 {
        with_reactive(|r| {
            let next = r
                .model_visual_subscriptions
                .checked_add(1)
                .expect("model visual subscription capacity exhausted");
            r.dirty_visual_widgets.reserve(next);
            r.model_visual_subscriptions = next;
        });
        let id = self.next_subscription.get();
        self.next_subscription.set(
            id.checked_add(1)
                .expect("model subscription identity exhausted"),
        );
        self.subscribers.borrow_mut().push(ModelSubscriber {
            id: Some(id),
            subscriber: OwnedSubscriber {
                world: Some(world),
                subscriber: Subscriber::VisualWidget(entity),
            },
        });
        id
    }

    pub(crate) fn unsubscribe(&self, id: u64) {
        let mut subscribers = self.subscribers.borrow_mut();
        let before = subscribers.len();
        subscribers.retain(|entry| entry.id != Some(id));
        if subscribers.len() != before {
            try_with_reactive(|r| r.model_visual_subscriptions -= 1);
        }
    }

    pub fn notify(&self) {
        let len = self.subscribers.borrow().len();
        for index in 0..len {
            let subscriber = self.subscribers.borrow()[index].subscriber;
            propagate(subscriber);
        }
    }
}

impl Drop for ModelSource {
    fn drop(&mut self) {
        let remaining = self
            .subscribers
            .get_mut()
            .iter()
            .filter(|entry| entry.id.is_some())
            .count();
        if remaining != 0 {
            try_with_reactive(|r| r.model_visual_subscriptions -= remaining);
        }
    }
}

// A source changed, so this computed's cache is stale: flag it and propagate to
// its own subscribers. Pure data mutation (no recompute, no closure) — the
// actual recompute is lazy, deferred to the next get(). Pulls subscribers out
// before recursing so a nested computed chain can't hold a borrow across calls.
fn mark_computed_dirty(id: ComputedId) {
    let node = with_reactive(|r| r.computeds.get(&id).and_then(Weak::upgrade));
    let Some(node) = node else {
        try_with_reactive(|r| {
            if r.computeds.remove(&id).is_some() {
                r.computed_slots.release(id.0);
            }
        });
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
    owner_world: Option<WorldId>,
}

fn unregister_effect(id: EffectId) {
    let removed = try_with_reactive(|r| {
        let removed = r.effects.remove(&id);
        if removed.is_some() {
            r.effect_slots.release(id.0);
        }
        removed
    });
    drop(removed);
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
        let owner_world = with_reactive(|r| r.world_id);
        assert!(
            owner_entity.is_none() || owner_world.is_some(),
            "widget effects require a world scope"
        );
        let inner = Rc::new(RefCell::new(EffectInner {
            run: Rc::new(f),
            owner_entity,
            owner_world,
        }));
        let id = with_reactive(|r| {
            let id = EffectId(r.effect_slots.allocate());
            r.effects.insert(id, inner);
            id
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
        unregister_effect(self.id);
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
    fn subscribers(&self) -> Vec<OwnedSubscriber>;
}

struct ComputedInner<T> {
    value: Option<T>,
    compute: alloc::boxed::Box<dyn Fn() -> T>,
    subscribers: Vec<OwnedSubscriber>,
    dirty: bool,
}

impl<T> ComputedNode for RefCell<ComputedInner<T>> {
    fn mark_dirty_take_was_dirty(&self) -> bool {
        let was = self.borrow().dirty;
        self.borrow_mut().dirty = true;
        was
    }

    fn subscribers(&self) -> Vec<OwnedSubscriber> {
        self.borrow().subscribers.clone()
    }
}

/// A lazily-recomputed derived value. Reads its sources through `get()`, so it
/// subscribes to them; when a source changes the cached value is invalidated
/// and recomputed on the next `get()`. Cheap to clone (shared handle).
pub struct Computed<T: 'static> {
    inner: Rc<RefCell<ComputedInner<T>>>,
    id: ComputedId,
    source_id: SourceId,
}

impl<T: 'static> Clone for Computed<T> {
    fn clone(&self) -> Self {
        Computed {
            inner: Rc::clone(&self.inner),
            id: self.id,
            source_id: self.source_id,
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
        let node: Rc<dyn ComputedNode> = inner.clone();
        let (id, source_id) = with_reactive(|r| {
            let id = ComputedId(r.computed_slots.allocate());
            let source_id = SourceId(r.source_slots.allocate());
            r.computeds.insert(id, Rc::downgrade(&node));
            (id, source_id)
        });
        Computed {
            inner,
            id,
            source_id,
        }
    }

    fn id(&self) -> ComputedId {
        self.id
    }

    pub fn get(&self) -> T
    where
        T: Clone,
    {
        let id = self.id();
        if let Some(sub) = current_scope() {
            let subscriber = OwnedSubscriber::tracked(sub);
            let mut inner = self.inner.borrow_mut();
            if !inner.subscribers.contains(&subscriber) {
                inner.subscribers.push(subscriber);
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

impl<T: 'static> Drop for Computed<T> {
    fn drop(&mut self) {
        if Rc::strong_count(&self.inner) == 1 {
            try_with_reactive(|r| {
                if r.computeds.remove(&self.id).is_some() {
                    r.computed_slots.release(self.id.0);
                }
                r.source_slots.release(self.source_id.0);
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // std test threads are pooled and reused, so the per-thread runtime can
    // carry residue between tests on the same thread — reset at entry.
    fn reset() {
        let effects = with_reactive(|r| {
            r.scope = None;
            r.world = core::ptr::null_mut();
            r.world_id = None;
            r.borrowed_worlds = [None; 8];
            r.dirty_widgets.clear();
            r.dirty_visual_widgets.clear();
            r.dirty_effects.clear();
            let effects = core::mem::take(&mut r.effects);
            for id in effects.keys() {
                r.effect_slots.release(id.0);
            }
            r.computeds.clear();
            effects
        });
        drop(effects);
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
            assert_eq!(r.dirty_widgets[0], (None, w));
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
            alloc::vec![OwnedSubscriber {
                world: None,
                subscriber: Subscriber::Widget(outer),
            }]
        );
        assert_eq!(
            s_inner.inner.borrow().subscribers,
            alloc::vec![OwnedSubscriber {
                world: None,
                subscriber: Subscriber::Widget(inner),
            }]
        );
    }

    #[test]
    fn tracking_scope_restores_after_unwind() {
        reset();
        let outer = Subscriber::Widget(entity(10));
        let inner = Subscriber::Widget(entity(11));
        with_scope(outer, || {
            let failed = std::panic::catch_unwind(|| {
                with_scope(inner, || panic!("scope unwind"));
            });
            assert!(failed.is_err());
            assert_eq!(current_scope(), Some(outer));
        });
        assert_eq!(current_scope(), None);
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
        let mut world = World::new();
        let entity = world.spawn_empty();
        let owned = with_world_scope(&mut world, || Effect::spawn(|| {}, Some(entity)));
        assert_eq!(owned.owner(), Some(entity));
        let standalone = Effect::new(|| {});
        assert_eq!(standalone.owner(), None);
    }

    #[test]
    fn effect_and_computed_slot_reuse_changes_generation() {
        reset();
        let effect = Effect::new(|| {});
        let first_effect = effect.id;
        drop(effect);
        let effect = Effect::new(|| {});
        assert_eq!(first_effect.0.slot, effect.id.0.slot);
        assert_ne!(first_effect.0.generation, effect.id.0.generation);
        let computed = Computed::new(|| 1u8);
        let first_computed = computed.id();
        drop(computed);
        let computed = Computed::new(|| 2u8);
        assert_eq!(first_computed.0.slot, computed.id().0.slot);
        assert_ne!(first_computed.0.generation, computed.id().0.generation);
    }

    #[test]
    fn source_slot_reuse_rejects_stale_signal_identity() {
        reset();
        let signal = Signal::new(1u8);
        let first = signal.inner.borrow().id;
        drop(signal);
        let signal = Signal::new(2u8);
        let second = signal.inner.borrow().id;
        assert_eq!(first.0.slot, second.0.slot);
        assert_ne!(first.0.generation, second.0.generation);
        assert!(!with_reactive(|r| r.source_slots.is_live(first.0)));
        assert!(with_reactive(|r| r.source_slots.is_live(second.0)));
    }

    #[test]
    fn disposing_effect_releases_captured_signal_after_registry_borrow() {
        reset();
        let signal = Signal::new(1u8);
        let captured = signal.clone();
        let effect = Effect::new(move || {
            let _ = captured.get();
        });
        drop(signal);
        effect.dispose();
        assert_eq!(effect_count(), 0);
    }

    #[test]
    fn matching_entities_from_different_worlds_keep_distinct_effect_owners() {
        reset();
        let mut a = World::new();
        let mut b = World::new();
        assert_ne!(a.id(), b.id());
        let entity_a = a.spawn_empty();
        let entity_b = b.spawn_empty();
        assert_eq!(entity_a, entity_b);
        with_world_scope(&mut a, || effect_with_widget(entity_a, || {}));
        with_world_scope(&mut b, || effect_with_widget(entity_b, || {}));
        assert_eq!(effect_count(), 2);
        cleanup_effects_for(&a, entity_a);
        assert_eq!(effect_count(), 1);
        with_world_scope(&mut a, || effect_with_widget(entity_a, || {}));
        a.despawn(entity_a);
        flush_signal_dirty(&mut a);
        assert_eq!(effect_count(), 1);
        flush_signal_dirty(&mut b);
        assert_eq!(effect_count(), 1);
        b.despawn(entity_b);
        flush_signal_dirty(&mut b);
        assert_eq!(effect_count(), 0);
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
                r.dirty_widgets.contains(&(None, w)),
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

    #[test]
    fn world_scope_restores_after_unwind() {
        reset();
        let mut outer = World::new();
        let mut inner = World::new();
        let outer_id = outer.id();
        with_world_scope(&mut outer, || {
            let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                with_world_scope(&mut inner, || panic!("world unwind"));
            }));
            assert!(failed.is_err());
            assert_eq!(current_world_id(), Some(outer_id));
        });
        assert_eq!(current_world_id(), None);
    }

    #[test]
    fn world_mutable_borrow_is_released_after_unwind() {
        reset();
        let mut world = World::new();
        let mut other = World::new();
        with_world_scope(&mut world, || {
            let failed = std::panic::catch_unwind(|| {
                with_world(|_| panic!("borrow unwind"));
            });
            assert!(failed.is_err());
            assert!(with_world(|_| ()).is_some());
            with_world(|_| {
                let reentrant = std::panic::catch_unwind(|| {
                    with_world(|_| ());
                });
                assert!(reentrant.is_err());
                with_world_scope(&mut other, || {
                    assert!(with_world(|_| ()).is_some());
                });
            });
        });
    }

    #[test]
    fn shared_signal_keeps_same_numbered_widgets_in_their_worlds() {
        reset();
        let mut a = World::new();
        let mut b = World::new();
        let a_widget = a.spawn_empty();
        let b_widget = b.spawn_empty();
        assert_eq!(a_widget, b_widget);
        let signal = Signal::new(0);
        let _a_subscription = signal.subscribe_widget(a.id(), a_widget);
        let _b_subscription = signal.subscribe_widget(b.id(), b_widget);

        signal.set(1);
        flush_signal_dirty(&mut a);
        assert!(a.get::<Dirty>(a_widget).is_some());
        assert!(b.get::<Dirty>(b_widget).is_none());
        flush_signal_dirty(&mut b);
        assert!(b.get::<Dirty>(b_widget).is_some());
    }

    #[test]
    fn flushing_one_world_preserves_other_worlds_effects() {
        reset();
        let mut a = World::new();
        let mut b = World::new();
        let signal = Signal::new(0);
        let a_runs = Rc::new(RefCell::new(0));
        let b_runs = Rc::new(RefCell::new(0));
        let a_signal = signal.clone();
        let b_signal = signal.clone();
        let a_count = Rc::clone(&a_runs);
        let b_count = Rc::clone(&b_runs);
        let _a_effect = with_world_scope(&mut a, || {
            Effect::new(move || {
                let _ = a_signal.get();
                *a_count.borrow_mut() += 1;
            })
        });
        let _b_effect = with_world_scope(&mut b, || {
            Effect::new(move || {
                let _ = b_signal.get();
                *b_count.borrow_mut() += 1;
            })
        });
        assert_eq!((*a_runs.borrow(), *b_runs.borrow()), (1, 1));

        signal.set(1);
        flush_signal_dirty(&mut a);
        assert_eq!((*a_runs.borrow(), *b_runs.borrow()), (2, 1));
        flush_signal_dirty(&mut b);
        assert_eq!((*a_runs.borrow(), *b_runs.borrow()), (2, 2));
    }

    #[test]
    fn shared_computed_notifies_widgets_in_each_world() {
        reset();
        let mut a = World::new();
        let mut b = World::new();
        let a_widget = a.spawn_empty();
        let b_widget = b.spawn_empty();
        let signal = Signal::new(1);
        let source = signal.clone();
        let computed = Computed::new(move || source.get() * 2);
        with_world_scope(&mut a, || {
            with_scope(Subscriber::Widget(a_widget), || {
                assert_eq!(computed.get(), 2)
            });
        });
        with_world_scope(&mut b, || {
            with_scope(Subscriber::Widget(b_widget), || {
                assert_eq!(computed.get(), 2)
            });
        });

        signal.set(2);
        flush_signal_dirty(&mut b);
        assert!(b.get::<Dirty>(b_widget).is_some());
        assert!(a.get::<Dirty>(a_widget).is_none());
        flush_signal_dirty(&mut a);
        assert!(a.get::<Dirty>(a_widget).is_some());
    }

    #[test]
    fn shared_visual_signal_preserves_other_worlds_pending_visuals() {
        reset();
        let mut a = World::new();
        let mut b = World::new();
        let a_widget = a.spawn_empty();
        let b_widget = b.spawn_empty();
        let signal = Signal::new(0);
        let _a_subscription = signal.subscribe_visual_widget(a.id(), a_widget);
        let _b_subscription = signal.subscribe_visual_widget(b.id(), b_widget);

        signal.set(1);
        flush_signal_dirty(&mut a);
        assert!(a.get::<VisualDirty>(a_widget).is_some());
        assert!(b.get::<VisualDirty>(b_widget).is_none());
        flush_signal_dirty(&mut b);
        assert!(b.get::<VisualDirty>(b_widget).is_some());
    }

    #[test]
    fn ownerless_effect_runs_on_the_first_world_flush() {
        reset();
        let mut a = World::new();
        let mut b = World::new();
        let signal = Signal::new(0);
        let source = signal.clone();
        let runs = Rc::new(RefCell::new(0));
        let count = Rc::clone(&runs);
        let _effect = Effect::new(move || {
            let _ = source.get();
            *count.borrow_mut() += 1;
        });
        signal.set(1);
        flush_signal_dirty(&mut b);
        assert_eq!(*runs.borrow(), 2);
        flush_signal_dirty(&mut a);
        assert_eq!(*runs.borrow(), 2);
        signal.set(2);
        flush_signal_dirty(&mut a);
        assert_eq!(*runs.borrow(), 3);
        assert_eq!(signal.inner.borrow().subscribers.len(), 1);
    }
}
