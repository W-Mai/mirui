extern crate alloc;

mod graph;
mod identity;

use alloc::collections::{BTreeMap, VecDeque};
use alloc::rc::{Rc, Weak};
use alloc::vec::Vec;
use core::cell::{Cell, RefCell};

use crate::ecs::world::WorldId;
use crate::ecs::{Entity, World};
use crate::ui::dirty::{Dirty, VisualDirty};
use graph::{ConsumerKey, PartitionKey, PendingKind, ReactiveGraph, SourceKey};
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
    scope_graph_consumer: Option<ConsumerKey>,
    // non-null only while an effect runs; lets a Fn() closure reach the World
    world: *mut World,
    world_id: Option<WorldId>,
    world_lifetime: Option<Weak<()>>,
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
    graph: ReactiveGraph,
}

impl Reactive {
    const fn new() -> Self {
        Reactive {
            scope: None,
            scope_graph_consumer: None,
            world: core::ptr::null_mut(),
            world_id: None,
            world_lifetime: None,
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
            graph: ReactiveGraph::new(),
        }
    }

    fn unbound_effect_count(&self) -> usize {
        self.effects
            .values()
            .filter(|effect| effect.borrow().owner_world.is_none())
            .count()
    }

    fn reserve_owned_consumer_slot(&mut self) {
        let needed = self
            .unbound_effect_count()
            .checked_add(1)
            .expect("reactive owner consumer headroom overflow");
        self.graph
            .reserve_owner_consumer_headroom(needed)
            .expect("reactive owner consumer capacity exhausted");
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

pub(crate) fn current_model_read_owner() -> Option<WorldId> {
    with_reactive(|r| {
        assert!(
            r.world_id.is_some() || !matches!(r.scope, Some(Subscriber::Computed(_))),
            "ownerless computed cannot read a registered model"
        );
        r.world_id
    })
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

struct ScopeGuard {
    previous_scope: Option<Subscriber>,
    previous_graph_consumer: Option<ConsumerKey>,
}

impl Drop for ScopeGuard {
    fn drop(&mut self) {
        with_reactive(|r| {
            r.scope = self.previous_scope;
            r.scope_graph_consumer = self.previous_graph_consumer;
        });
    }
}

/// Run `f` with `scope` as the active reactive consumer, restoring the
/// previous scope after. Signals read inside `f` subscribe to `scope`.
/// Each owned Widget or VisualWidget scope is one complete evaluation: a later
/// outermost evaluation replaces its previous dependencies. Group related
/// reads in one scope; a nested scope for the same target extends that evaluation.
pub(crate) fn with_scope<R>(scope: Subscriber, f: impl FnOnce() -> R) -> R {
    let _guard = with_reactive(|r| {
        let graph_consumer = match (r.world_id, scope) {
            (Some(world), Subscriber::Widget(_) | Subscriber::VisualWidget(_)) => {
                let partition = r
                    .graph
                    .owner_key(world)
                    .expect("Widget owner is not registered");
                if r.scope == Some(scope)
                    && r.scope_graph_consumer
                        .is_some_and(|consumer| consumer.partition == partition)
                {
                    r.scope_graph_consumer
                } else if let Some(consumer) = r.graph.find_consumer(partition, scope) {
                    r.graph
                        .clear_consumer(consumer)
                        .expect("live Widget graph consumer missing before evaluation");
                    Some(consumer)
                } else {
                    r.reserve_owned_consumer_slot();
                    match scope {
                        Subscriber::Widget(_) => {
                            let count = r.graph.count_consumers_matching(|target| {
                                matches!(target, Subscriber::Widget(_))
                            });
                            r.dirty_widgets.reserve(count + 1);
                        }
                        Subscriber::VisualWidget(_) => {
                            let count = r.graph.count_consumers_matching(|target| {
                                matches!(target, Subscriber::VisualWidget(_))
                            });
                            r.dirty_visual_widgets
                                .reserve(count + r.model_visual_subscriptions + 1);
                        }
                        _ => unreachable!("only Widget scopes enter this branch"),
                    }
                    Some(
                        r.graph
                            .register_consumer(partition, scope)
                            .expect("reactive Widget consumer registration failed"),
                    )
                }
            }
            _ => None,
        };
        ScopeGuard {
            previous_scope: r.scope.replace(scope),
            previous_graph_consumer: core::mem::replace(
                &mut r.scope_graph_consumer,
                graph_consumer,
            ),
        }
    });
    f()
}

fn enqueue_widget(owner: Option<WorldId>, entity: Entity) {
    with_reactive(|r| {
        if !r.dirty_widgets.contains(&(owner, entity)) {
            r.dirty_widgets.push_back((owner, entity));
        }
    });
}

fn enqueue_visual_widget(owner: Option<WorldId>, entity: Entity) {
    with_reactive(|r| {
        if !r.dirty_visual_widgets.contains(&(owner, entity)) {
            r.dirty_visual_widgets.push_back((owner, entity));
        }
    });
}

fn enqueue_effect(id: EffectId) {
    with_reactive(|r| {
        if !r.dirty_effects.contains(&id) {
            r.dirty_effects.push_back(id);
        }
    });
}

// Effects created outside a World may bind to one model owner on their first
// model read. Shared sources keep using the original shared consumer.
fn subscribe_automatic_graph(source: SourceKey, subscriber: Subscriber) -> bool {
    match subscriber {
        Subscriber::Effect(id) => {
            with_reactive(|r| {
                let Some(effect) = r.effects.get(&id) else {
                    return;
                };
                let mut effect = effect.borrow_mut();
                let consumer = match (source.partition, effect.graph_consumer.partition) {
                    (PartitionKey::Owner { world, .. }, PartitionKey::Shared) => {
                        if let Some(bound) = effect.owner_world {
                            assert_eq!(bound, world, "Effect cannot read models from two Apps");
                        }
                        if let Some(consumer) = effect.model_consumer {
                            consumer
                        } else {
                            let consumer = r
                                .graph
                                .register_consumer(source.partition, subscriber)
                                .expect("reactive model Effect reserved capacity exhausted");
                            effect.model_consumer = Some(consumer);
                            effect.owner_world = Some(world);
                            consumer
                        }
                    }
                    _ => effect.graph_consumer,
                };
                drop(effect);
                r.graph
                    .subscribe(source, consumer)
                    .expect("reactive Effect dependency subscription failed");
            });
            true
        }
        Subscriber::Computed(id) => {
            with_reactive(|r| {
                if let Some(node) = r.computeds.get(&id).and_then(Weak::upgrade) {
                    r.graph
                        .subscribe(source, node.graph_consumer())
                        .expect("reactive Computed dependency subscription failed");
                }
            });
            true
        }
        Subscriber::Widget(_) | Subscriber::VisualWidget(_) => with_reactive(|r| {
            let Some(consumer) = r.scope_graph_consumer else {
                return false;
            };
            r.graph
                .subscribe(source, consumer)
                .expect("reactive Widget dependency subscription failed");
            true
        }),
    }
}

// Graph publication only queues effects/widgets and marks Computeds dirty.
// Computed propagation runs after the runtime borrow is released; evaluation
// closures remain lazy and run only from Computed::get.
fn publish_graph_source(source: SourceKey) {
    with_reactive(|r| {
        r.graph
            .publish(source)
            .expect("reactive graph publication failed");
        while let Some((consumer, target)) = r.graph.take_pending_any_kind(PendingKind::NonComputed)
        {
            match target {
                Subscriber::Effect(effect) => {
                    if !r.dirty_effects.contains(&effect) {
                        r.dirty_effects.push_back(effect);
                    }
                }
                Subscriber::Widget(entity) => {
                    let owner = match consumer.partition {
                        PartitionKey::Shared => None,
                        PartitionKey::Owner { world, .. } => Some(world),
                    };
                    if !r.dirty_widgets.contains(&(owner, entity)) {
                        r.dirty_widgets.push_back((owner, entity));
                    }
                }
                Subscriber::VisualWidget(entity) => {
                    let owner = match consumer.partition {
                        PartitionKey::Shared => None,
                        PartitionKey::Owner { world, .. } => Some(world),
                    };
                    if !r.dirty_visual_widgets.contains(&(owner, entity)) {
                        r.dirty_visual_widgets.push_back((owner, entity));
                    }
                }
                Subscriber::Computed(_) => unreachable!("computed filtered from graph queue"),
            }
        }
    });
    while let Some((_, Subscriber::Computed(computed))) =
        with_reactive(|r| r.graph.take_pending_computed())
    {
        mark_computed_dirty(computed);
    }
}

fn register_graph_owner(world: WorldId) {
    with_reactive(|r| {
        if r.graph.owner_key(world).is_ok() {
            return;
        }
        r.graph
            .reserve_owner_consumer_headroom(r.unbound_effect_count())
            .expect("reactive owner Effect headroom exhausted");
        r.graph
            .reserve_for_owner()
            .expect("reactive owner graph capacity exhausted");
        r.graph
            .add_owner(world)
            .expect("reactive owner registration failed");
    });
}

#[cfg(test)]
pub(crate) fn has_graph_owner(world: WorldId) -> bool {
    with_reactive(|r| r.graph.owner_key(world).is_ok())
}

/// Release graph state and widget-bound effects for a finished World.
pub(crate) fn release_graph_owner(world: WorldId) {
    loop {
        let effect = try_with_reactive(|r| {
            r.effects.iter().find_map(|(id, effect)| {
                (effect.borrow().owner_world == Some(world)).then_some(*id)
            })
        })
        .flatten();
        let Some(effect) = effect else { break };
        unregister_effect(effect);
    }
    try_with_reactive(|r| {
        r.graph.remove_owner(world);
        r.dirty_widgets.retain(|(owner, _)| *owner != Some(world));
        r.dirty_visual_widgets
            .retain(|(owner, _)| *owner != Some(world));
    });
}

struct ReactiveOwnerCleanup;

pub(crate) struct OwnerGuard {
    prev: Option<WorldId>,
    prev_lifetime: Option<Weak<()>>,
}

impl OwnerGuard {
    pub(crate) fn enter(world: &mut World) -> Self {
        world.register_drop_hook::<ReactiveOwnerCleanup>(release_graph_owner);
        Self::enter_id(Some(world.id()), Some(world.lifetime()))
    }

    fn enter_id(owner: Option<WorldId>, lifetime: Option<Weak<()>>) -> Self {
        if let (Some(owner), Some(lifetime)) = (owner, lifetime.as_ref()) {
            assert!(lifetime.strong_count() > 0, "reactive owner has ended");
            register_graph_owner(owner);
        }
        let (prev, prev_lifetime) = with_reactive(|r| {
            (
                core::mem::replace(&mut r.world_id, owner),
                core::mem::replace(&mut r.world_lifetime, lifetime),
            )
        });
        Self {
            prev,
            prev_lifetime,
        }
    }
}

impl Drop for OwnerGuard {
    fn drop(&mut self) {
        with_reactive(|r| {
            r.world_id = self.prev;
            r.world_lifetime = self.prev_lifetime.take();
        });
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

// An ownerless Effect cannot safely use whichever World happened to flush
// first. Hide that ambient World until the Effect reads a model and binds to
// the model's owner; no user closure runs under the runtime borrow.
struct WorldlessEffectGuard {
    previous_world: *mut World,
    previous_owner: Option<WorldId>,
    previous_lifetime: Option<Weak<()>>,
}

impl WorldlessEffectGuard {
    fn enter() -> Self {
        let (previous_world, previous_owner, previous_lifetime) = with_reactive(|r| {
            (
                core::mem::replace(&mut r.world, core::ptr::null_mut()),
                r.world_id.take(),
                r.world_lifetime.take(),
            )
        });
        Self {
            previous_world,
            previous_owner,
            previous_lifetime,
        }
    }
}

impl Drop for WorldlessEffectGuard {
    fn drop(&mut self) {
        with_reactive(|r| {
            r.world = self.previous_world;
            r.world_id = self.previous_owner;
            r.world_lifetime = self.previous_lifetime.take();
        });
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
/// An ownerless Effect returns None even when a World flushes it. The run that
/// binds it to a model also remains ownerless; later runs in the model's World
/// can access that World.
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

fn pop_widget_for(
    queue: &mut VecDeque<(Option<WorldId>, Entity)>,
    world_id: WorldId,
) -> Option<Entity> {
    let (owner, entity) = queue.pop_front().expect("queued widget");
    if owner.is_none_or(|owner| owner == world_id) {
        Some(entity)
    } else {
        queue.push_back((owner, entity));
        None
    }
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
        let pending_effects = with_reactive(|r| r.dirty_effects.len());
        let mut ran_effect = false;
        for _ in 0..pending_effects {
            let effect = with_reactive(|r| {
                let id = r.dirty_effects.pop_front().expect("queued effect");
                match r.effects.get(&id).map(|effect| effect.borrow().owner_world) {
                    Some(Some(owner)) if owner != world_id => {
                        r.dirty_effects.push_back(id);
                        None
                    }
                    Some(_) => Some(id),
                    None => None,
                }
            });
            if let Some(effect) = effect {
                run_effect(effect);
                ran_effect = true;
            }
        }
        let pending_widgets = with_reactive(|r| r.dirty_widgets.len());
        let mut marked_widget = false;
        for _ in 0..pending_widgets {
            let entity = with_reactive(|r| pop_widget_for(&mut r.dirty_widgets, world_id));
            if let Some(entity) = entity {
                marked_widget = true;
                if world.is_alive(entity) {
                    world.insert(entity, Dirty);
                }
            }
        }
        let pending_visual_widgets = with_reactive(|r| r.dirty_visual_widgets.len());
        let mut marked_visual_widget = false;
        for _ in 0..pending_visual_widgets {
            let entity = with_reactive(|r| pop_widget_for(&mut r.dirty_visual_widgets, world_id));
            if let Some(entity) = entity {
                marked_visual_widget = true;
                if world.is_alive(entity) {
                    world.insert(entity, VisualDirty);
                }
            }
        }
        if !ran_effect && !marked_widget && !marked_visual_widget {
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
    loop {
        let dead = with_reactive(|r| {
            let partition = r.graph.owner_key(world.id()).ok()?;
            r.graph
                .find_consumer_matching(partition, |target| match target {
                    Subscriber::Widget(entity) | Subscriber::VisualWidget(entity) => {
                        !world.is_alive(entity)
                    }
                    _ => false,
                })
        });
        let Some(dead) = dead else { break };
        with_reactive(|r| {
            r.graph
                .remove_consumer(dead)
                .expect("dead Widget graph consumer missing during cleanup");
            if r.scope_graph_consumer == Some(dead) {
                r.scope_graph_consumer = None;
            }
        });
    }
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
    with_reactive(|r| {
        let Ok(partition) = r.graph.owner_key(world.id()) else {
            return;
        };
        for target in [Subscriber::Widget(entity), Subscriber::VisualWidget(entity)] {
            if let Some(consumer) = r.graph.find_consumer(partition, target) {
                r.graph
                    .remove_consumer(consumer)
                    .expect("Widget graph consumer missing during teardown");
                if r.scope_graph_consumer == Some(consumer) {
                    r.scope_graph_consumer = None;
                }
            }
        }
    });
}

struct SignalInner<T> {
    id: SourceId,
    graph_source: SourceKey,
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
            let inner = self.inner.borrow();
            let id = inner.id;
            let graph_source = inner.graph_source;
            try_with_reactive(|r| {
                r.graph
                    .remove_source(graph_source)
                    .expect("live Signal graph source missing on drop");
                r.source_slots.release(id.0);
            });
        }
    }
}

impl<T: 'static> Signal<T> {
    pub fn new(initial: T) -> Self {
        let (id, graph_source) = with_reactive(|r| {
            let id = SourceId(r.source_slots.allocate());
            let source = r
                .graph
                .reserve_for_shared_source()
                .and_then(|_| r.graph.register_shared_source());
            let graph_source = match source {
                Ok(source) => source,
                Err(error) => {
                    r.source_slots.release(id.0);
                    panic!("reactive shared source registration failed: {error:?}");
                }
            };
            (id, graph_source)
        });
        Signal {
            inner: Rc::new(RefCell::new(SignalInner {
                id,
                graph_source,
                value: initial,
                subscribers: Vec::new(),
            })),
        }
    }

    fn track(&self) {
        if let Some(sub) = current_scope() {
            let source = self.inner.borrow().graph_source;
            if !subscribe_automatic_graph(source, sub) {
                self.add_subscriber(OwnedSubscriber::tracked(sub));
            }
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
        let inner = self.inner.borrow();
        let id = inner.id;
        let source = inner.graph_source;
        debug_assert!(with_reactive(|r| r.source_slots.is_live(id.0)));
        publish_graph_source(source);
        for &sub in &inner.subscribers {
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
    subscribers: RefCell<Vec<OwnedSubscriber>>,
    graph_source: Cell<Option<SourceKey>>,
}

/// One graph consumer owned by a View observation binding.
pub(crate) struct ModelVisualSubscription {
    consumer: ConsumerKey,
}

impl Drop for ModelVisualSubscription {
    fn drop(&mut self) {
        try_with_reactive(|r| {
            let _ = r.graph.remove_consumer(self.consumer);
        });
    }
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
            graph_source: Cell::new(None),
        }
    }

    pub(crate) fn register_owner(&self, owner: WorldId) {
        if let Some(source) = self.graph_source.get() {
            assert!(
                matches!(source.partition, PartitionKey::Owner { world, .. } if world == owner),
                "model source is already registered to another App"
            );
            return;
        }
        register_graph_owner(owner);
        let source = with_reactive(|r| {
            r.graph
                .reserve_for_owner_source(owner)
                .expect("reactive model source capacity exhausted");
            r.graph
                .register_owner_source(owner)
                .expect("reactive model source registration failed")
        });
        self.graph_source.set(Some(source));
    }

    pub fn track(&self) {
        let Some(subscriber) = current_scope() else {
            return;
        };
        if let Some(source) = self.graph_source.get()
            && subscribe_automatic_graph(source, subscriber)
        {
            return;
        }
        let subscriber = OwnedSubscriber::tracked(subscriber);
        let mut subscribers = self.subscribers.borrow_mut();
        if !subscribers.contains(&subscriber) {
            if matches!(subscriber.subscriber, Subscriber::VisualWidget(_)) {
                with_reactive(|r| {
                    let next = r
                        .model_visual_subscriptions
                        .checked_add(1)
                        .expect("model visual subscription capacity exhausted");
                    let graph_visuals = r.graph.count_consumers_matching(|target| {
                        matches!(target, Subscriber::VisualWidget(_))
                    });
                    r.dirty_visual_widgets.reserve(next + graph_visuals);
                    r.model_visual_subscriptions = next;
                });
            }
            subscribers.push(subscriber);
        }
    }

    pub(crate) fn subscribe_visual_widget(
        &self,
        world: WorldId,
        entity: Entity,
    ) -> ModelVisualSubscription {
        let source = self
            .graph_source
            .get()
            .expect("visual model source must be registered");
        let consumer = with_reactive(|r| {
            let partition = r
                .graph
                .owner_key(world)
                .expect("View owner is not registered");
            assert_eq!(
                source.partition, partition,
                "model belongs to a different App"
            );
            r.reserve_owned_consumer_slot();
            let graph_visuals = r
                .graph
                .count_consumers_matching(|target| matches!(target, Subscriber::VisualWidget(_)));
            r.dirty_visual_widgets
                .reserve(r.model_visual_subscriptions + graph_visuals + 1);
            let consumer = r
                .graph
                .register_explicit_visual_consumer(partition, entity)
                .expect("reactive View consumer registration failed");
            if let Err(error) = r.graph.subscribe(source, consumer) {
                let _ = r.graph.remove_consumer(consumer);
                panic!("reactive View dependency subscription failed: {error:?}");
            }
            consumer
        });
        ModelVisualSubscription { consumer }
    }

    pub fn notify(&self) {
        if let Some(source) = self.graph_source.get() {
            publish_graph_source(source);
        }
        let len = self.subscribers.borrow().len();
        for index in 0..len {
            let subscriber = self.subscribers.borrow()[index];
            propagate(subscriber);
        }
    }

    pub(crate) fn invalidate_computeds(&self) {
        if let Some(source) = self.graph_source.get() {
            invalidate_computed_source(source);
        }
    }
}

impl Drop for ModelSource {
    fn drop(&mut self) {
        self.invalidate_computeds();
        let source = self.graph_source.get();
        let remaining_legacy_visuals = self
            .subscribers
            .get_mut()
            .iter()
            .filter(|entry| matches!(entry.subscriber, Subscriber::VisualWidget(_)))
            .count();
        if remaining_legacy_visuals != 0 || source.is_some() {
            try_with_reactive(|r| {
                if let Some(source) = source {
                    let _ = r.graph.remove_source(source);
                }
                r.model_visual_subscriptions -= remaining_legacy_visuals;
            });
        }
    }
}

// A removed model source must invalidate cached values, but scheduling Effects
// or Widgets would execute callbacks against a registration that no longer
// exists. Traverse only Computed consumers through the graph's reserved queue.
fn invalidate_computed_source(source: SourceKey) {
    if !queue_computed_invalidation(source) {
        return;
    }
    while let Some((_, Subscriber::Computed(id))) =
        try_with_reactive(|r| r.graph.take_pending_computed()).flatten()
    {
        let node = try_with_reactive(|r| r.computeds.get(&id).and_then(Weak::upgrade)).flatten();
        let Some(node) = node else {
            try_with_reactive(|r| {
                if r.computeds.remove(&id).is_some() {
                    r.computed_slots.release(id.0);
                }
            });
            continue;
        };
        if !node.mark_dirty_take_was_dirty() {
            queue_computed_invalidation(node.graph_source());
        }
    }
}

fn queue_computed_invalidation(source: SourceKey) -> bool {
    match try_with_reactive(|r| r.graph.publish_computed_only(source)) {
        Some(Ok(())) => true,
        Some(Err(graph::GraphError::StaleSource)) | None => false,
        Some(Err(error)) => panic!("reactive computed invalidation failed: {error:?}"),
    }
}

// A source changed, so this computed's cache is stale: flag it and propagate to
// its own subscribers. Pure data mutation (no recompute, no closure) — the
// actual recompute is lazy, deferred to the next get(). Propagation only queues
// work or marks downstream computed nodes; it does not run user closures.
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
    publish_graph_source(node.graph_source());
    node.propagate_subscribers();
}

struct EffectInner {
    run: Rc<dyn Fn()>,
    owner_entity: Option<Entity>,
    owner_world: Option<WorldId>,
    graph_consumer: ConsumerKey,
    model_consumer: Option<ConsumerKey>,
}

fn unregister_effect(id: EffectId) {
    let removed = try_with_reactive(|r| {
        let removed = r.effects.remove(&id);
        if let Some(effect) = &removed {
            let effect = effect.borrow();
            if let Some(consumer) = effect.model_consumer {
                let _ = r.graph.remove_consumer(consumer);
            }
            let _ = r.graph.remove_consumer(effect.graph_consumer);
            r.dirty_effects.retain(|queued| *queued != id);
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

struct EffectInitGuard {
    id: EffectId,
    initialized: bool,
}

impl Drop for EffectInitGuard {
    fn drop(&mut self) {
        if !self.initialized {
            unregister_effect(self.id);
        }
    }
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
        let run: Rc<dyn Fn()> = Rc::new(f);
        let id = with_reactive(|r| {
            let partition = match owner_world {
                Some(world) => r
                    .graph
                    .owner_key(world)
                    .expect("Effect owner is not registered"),
                None => PartitionKey::Shared,
            };
            if owner_world.is_some() {
                r.reserve_owned_consumer_slot();
            } else {
                let needed = r
                    .unbound_effect_count()
                    .checked_add(1)
                    .expect("reactive owner consumer headroom overflow");
                r.graph
                    .reserve_owner_consumer_headroom(needed)
                    .expect("reactive unbound Effect headroom exhausted");
                r.graph
                    .reserve_for_consumer(partition)
                    .expect("reactive shared Effect consumer capacity exhausted");
            }
            let id = EffectId(r.effect_slots.allocate());
            let graph_consumer = r
                .graph
                .register_consumer(partition, Subscriber::Effect(id))
                .expect("reactive Effect registration failed");
            let inner = Rc::new(RefCell::new(EffectInner {
                run,
                owner_entity,
                owner_world,
                graph_consumer,
                model_consumer: None,
            }));
            r.effects.insert(id, inner);
            r.dirty_effects.reserve(r.effects.len());
            id
        });
        let mut init = EffectInitGuard {
            id,
            initialized: false,
        };
        run_effect(id);
        init.initialized = true;
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
    let run = with_reactive(|r| {
        let effect = r.effects.get(&id)?;
        let (consumer, model_consumer, ownerless, run) = {
            let effect = effect.borrow();
            (
                effect.graph_consumer,
                effect.model_consumer,
                effect.owner_world.is_none(),
                Rc::clone(&effect.run),
            )
        };
        r.graph
            .clear_consumer(consumer)
            .expect("live Effect graph consumer missing before run");
        if let Some(model_consumer) = model_consumer {
            r.graph
                .clear_consumer(model_consumer)
                .expect("live model Effect consumer missing before run");
        }
        Some((run, ownerless))
    });
    if let Some((run, ownerless)) = run {
        let _worldless = ownerless.then(WorldlessEffectGuard::enter);
        with_scope(Subscriber::Effect(id), || run());
    }
}

// Type-erased view of a Computed so the runtime registry can hold mixed `T`
// without a generic. Only the non-generic propagation hooks are exposed.
trait ComputedNode {
    fn mark_dirty_take_was_dirty(&self) -> bool;
    fn propagate_subscribers(&self);
    fn graph_source(&self) -> SourceKey;
    fn graph_consumer(&self) -> ConsumerKey;
}

struct ComputedInner<T> {
    value: Option<T>,
    compute: alloc::boxed::Box<dyn Fn() -> T>,
    subscribers: Vec<OwnedSubscriber>,
    dirty: bool,
    owner_world: Option<WorldId>,
    owner_lifetime: Option<Weak<()>>,
    graph_source: SourceKey,
    graph_consumer: ConsumerKey,
}

impl<T> ComputedNode for RefCell<ComputedInner<T>> {
    fn mark_dirty_take_was_dirty(&self) -> bool {
        let was = self.borrow().dirty;
        self.borrow_mut().dirty = true;
        was
    }

    fn propagate_subscribers(&self) {
        let inner = self.borrow();
        for &subscriber in &inner.subscribers {
            propagate(subscriber);
        }
    }

    fn graph_source(&self) -> SourceKey {
        self.borrow().graph_source
    }

    fn graph_consumer(&self) -> ConsumerKey {
        self.borrow().graph_consumer
    }
}

/// A lazily-recomputed derived value. Reads its sources through `get()`, so it
/// subscribes to them; when a source changes the cached value is invalidated
/// and recomputed on the next `get()`. Cheap to clone (shared handle).
pub struct Computed<T: 'static> {
    inner: Rc<RefCell<ComputedInner<T>>>,
    id: ComputedId,
    source_id: SourceId,
    graph_source: SourceKey,
    graph_consumer: ConsumerKey,
}

impl<T: 'static> Clone for Computed<T> {
    fn clone(&self) -> Self {
        Computed {
            inner: Rc::clone(&self.inner),
            id: self.id,
            source_id: self.source_id,
            graph_source: self.graph_source,
            graph_consumer: self.graph_consumer,
        }
    }
}

impl<T: 'static> Computed<T> {
    pub fn new(f: impl Fn() -> T + 'static) -> Self {
        let owner_world = current_world_id();
        let owner_lifetime = with_reactive(|r| r.world_lifetime.clone());
        let (id, source_id, graph_source, graph_consumer) = with_reactive(|r| {
            let partition = match owner_world {
                Some(world) => r
                    .graph
                    .owner_key(world)
                    .expect("Computed owner is not registered"),
                None => PartitionKey::Shared,
            };
            match owner_world {
                Some(world) => r
                    .graph
                    .reserve_for_owner_source(world)
                    .expect("reactive Computed source capacity exhausted"),
                None => r
                    .graph
                    .reserve_for_shared_source()
                    .expect("reactive shared Computed source capacity exhausted"),
            }
            if owner_world.is_some() {
                r.reserve_owned_consumer_slot();
            } else {
                r.graph
                    .reserve_for_consumer(partition)
                    .expect("reactive shared Computed consumer capacity exhausted");
            }
            let id = ComputedId(r.computed_slots.allocate());
            let source_id = SourceId(r.source_slots.allocate());
            let graph_source = match owner_world {
                Some(world) => r.graph.register_owner_source(world),
                None => r.graph.register_shared_source(),
            }
            .expect("reactive Computed source registration failed");
            let graph_consumer = r
                .graph
                .register_consumer(partition, Subscriber::Computed(id))
                .expect("reactive Computed consumer registration failed");
            (id, source_id, graph_source, graph_consumer)
        });
        let inner = Rc::new(RefCell::new(ComputedInner {
            value: None,
            compute: alloc::boxed::Box::new(f),
            subscribers: Vec::new(),
            dirty: true,
            owner_world,
            owner_lifetime,
            graph_source,
            graph_consumer,
        }));
        let node: Rc<dyn ComputedNode> = inner.clone();
        with_reactive(|r| {
            r.computeds.insert(id, Rc::downgrade(&node));
        });
        Computed {
            inner,
            id,
            source_id,
            graph_source,
            graph_consumer,
        }
    }

    fn id(&self) -> ComputedId {
        self.id
    }

    pub fn get(&self) -> T
    where
        T: Clone,
    {
        let inner = self.inner.borrow();
        let owner = inner.owner_world;
        let lifetime = inner.owner_lifetime.clone();
        drop(inner);
        if let Some(lifetime) = &lifetime {
            assert!(lifetime.strong_count() > 0, "computed owner has ended");
        }
        if let (Some(active), Some(owner)) = (current_world_id(), owner) {
            assert_eq!(active, owner, "computed belongs to a different App");
        }
        let id = self.id();
        if let Some(sub) = current_scope() {
            if !subscribe_automatic_graph(self.graph_source, sub) {
                let subscriber = OwnedSubscriber::tracked(sub);
                let mut inner = self.inner.borrow_mut();
                if !inner.subscribers.contains(&subscriber) {
                    inner.subscribers.push(subscriber);
                }
            }
        }
        if self.inner.borrow().dirty {
            with_reactive(|r| {
                r.graph
                    .clear_consumer(self.graph_consumer)
                    .expect("live Computed consumer missing before evaluation");
            });
            // Recompute in this computed's scope so its sources subscribe IT,
            // not whatever outer consumer triggered the read.
            let _owner = OwnerGuard::enter_id(owner, lifetime);
            let _read_only = ModelReadOnlyGuard::enter();
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
                let _ = r.graph.remove_consumer(self.graph_consumer);
                let _ = r.graph.remove_source(self.graph_source);
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
            r.world_lifetime = None;
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
        with_reactive(|r| {
            r.graph = ReactiveGraph::new();
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
    fn stale_explicit_view_binding_cannot_unsubscribe_a_reused_consumer_slot() {
        reset();
        let mut app = crate::app::App::headless(32, 32);
        let source = ModelSource::new();
        source.register_owner(app.world.id());
        let entity = app.world.spawn_empty();

        let first = source.subscribe_visual_widget(app.world.id(), entity);
        let stale_key = first.consumer;
        drop(first);
        let second = source.subscribe_visual_widget(app.world.id(), entity);
        assert_eq!(second.consumer.slot.slot, stale_key.slot.slot);
        assert_ne!(second.consumer.slot.generation, stale_key.slot.generation);

        drop(ModelVisualSubscription {
            consumer: stale_key,
        });
        source.notify();
        flush_signal_dirty(&mut app.world);
        assert!(app.world.has::<VisualDirty>(entity));
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
    fn repeated_notifications_enqueue_a_widget_once_until_consumed() {
        reset();
        let signal = Signal::new(0_u8);
        let widget = entity(4);
        with_scope(Subscriber::Widget(widget), || {
            let _ = signal.get();
        });
        signal.set(1);
        signal.set(2);
        with_reactive(|r| {
            assert_eq!(r.dirty_widgets.len(), 1);
            assert_eq!(r.dirty_widgets.pop_front(), Some((None, widget)));
        });
        signal.set(3);
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
    fn panicking_initial_effect_run_unregisters_its_consumer() {
        reset();
        let signal = Signal::new(0u8);
        let source = signal.clone();
        let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _effect = Effect::new(move || {
                let _ = source.get();
                panic!("initial evaluation failed");
            });
        }));
        assert!(failure.is_err());
        with_reactive(|r| assert!(r.effects.is_empty()));
        let source = signal.inner.borrow().graph_source;
        let mut subscribers = 0;
        with_reactive(|r| {
            r.graph
                .for_each_subscriber(source, |_, _| subscribers += 1)
                .unwrap();
        });
        assert_eq!(subscribers, 0);
        signal.set(1);
        with_reactive(|r| assert!(r.dirty_effects.is_empty()));
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
    fn effect_switches_signal_dependencies_without_retaining_old_edges() {
        reset();
        let choose_right = Signal::new(false);
        let left = Signal::new(1u8);
        let right = Signal::new(10u8);
        let runs = Rc::new(Cell::new(0u32));
        let (choose_read, left_read, right_read, run_count) = (
            choose_right.clone(),
            left.clone(),
            right.clone(),
            Rc::clone(&runs),
        );
        let _effect = Effect::new(move || {
            if choose_read.get() {
                let _ = right_read.get();
            } else {
                let _ = left_read.get();
            }
            run_count.set(run_count.get() + 1);
        });
        assert_eq!(runs.get(), 1);

        left.set(2);
        drain_effects();
        assert_eq!(runs.get(), 2);
        right.set(11);
        drain_effects();
        assert_eq!(runs.get(), 2);

        choose_right.set(true);
        drain_effects();
        assert_eq!(runs.get(), 3);
        left.set(3);
        drain_effects();
        assert_eq!(runs.get(), 3, "the old branch must be unlinked");
        right.set(12);
        drain_effects();
        assert_eq!(runs.get(), 4);
    }

    #[test]
    fn ownerless_effect_switches_registered_model_sources_without_allocating() {
        reset();
        let mut world = World::new();
        drop(OwnerGuard::enter(&mut world));
        let owner = world.id();
        let first = Rc::new(ModelSource::new());
        let second = Rc::new(ModelSource::new());
        first.register_owner(owner);
        second.register_owner(owner);
        let use_first = Signal::new(true);
        let runs = Rc::new(Cell::new(0u32));
        let (branch, first_read, second_read, run_count) = (
            use_first.clone(),
            Rc::clone(&first),
            Rc::clone(&second),
            Rc::clone(&runs),
        );
        let _effect = Effect::new(move || {
            if branch.get() {
                first_read.track();
            } else {
                second_read.track();
            }
            run_count.set(run_count.get() + 1);
        });
        assert_eq!(runs.get(), 1);

        let allocations = graph::tests::tracked_allocations(|| {
            use_first.set(false);
            flush_signal_dirty(&mut world);
        });
        assert_eq!(
            allocations, 0,
            "registered branch switch must reuse graph storage"
        );
        assert_eq!(runs.get(), 2);
        first.notify();
        flush_signal_dirty(&mut world);
        assert_eq!(runs.get(), 2, "the old model source must be unlinked");
        second.notify();
        flush_signal_dirty(&mut world);
        assert_eq!(runs.get(), 3);
    }

    #[test]
    fn ownerless_effect_first_model_read_reuses_structural_consumer_capacity() {
        reset();
        let mut world = World::new();
        drop(OwnerGuard::enter(&mut world));
        let model = Rc::new(ModelSource::new());
        model.register_owner(world.id());
        let use_model = Signal::new(false);
        let runs = Rc::new(Cell::new(0u32));
        let (branch, model_read, run_count) =
            (use_model.clone(), Rc::clone(&model), Rc::clone(&runs));
        let _effect = Effect::new(move || {
            if branch.get() {
                model_read.track();
            }
            run_count.set(run_count.get() + 1);
        });
        assert_eq!(runs.get(), 1);

        let allocations = graph::tests::tracked_allocations(|| {
            use_model.set(true);
            flush_signal_dirty(&mut world);
        });
        assert_eq!(
            allocations, 0,
            "first model read must use reserved capacity"
        );
        assert_eq!(runs.get(), 2);
    }

    #[test]
    fn unbound_effect_retains_headroom_when_owner_and_widget_arrive_later() {
        reset();
        let gate = Signal::new(false);
        let model = Rc::new(ModelSource::new());
        let runs = Rc::new(Cell::new(0u32));
        let (branch, observed, count) = (gate.clone(), Rc::clone(&model), Rc::clone(&runs));
        let _effect = Effect::new(move || {
            if branch.get() {
                observed.track();
            }
            count.set(count.get() + 1);
        });
        let mut world = World::new();
        let widget = world.spawn_empty();
        with_world_scope(&mut world, || {
            with_scope(Subscriber::Widget(widget), || ());
        });
        model.register_owner(world.id());

        assert_eq!(
            graph::tests::tracked_allocations(|| {
                gate.set(true);
                flush_signal_dirty(&mut world);
            }),
            0
        );
        assert_eq!(runs.get(), 2);
    }

    #[test]
    fn ownerless_effect_cannot_subscribe_to_models_from_two_apps() {
        reset();
        let mut first_world = World::new();
        let mut second_world = World::new();
        drop(OwnerGuard::enter(&mut first_world));
        drop(OwnerGuard::enter(&mut second_world));
        let first = Rc::new(ModelSource::new());
        let second = Rc::new(ModelSource::new());
        first.register_owner(first_world.id());
        second.register_owner(second_world.id());
        let use_second = Signal::new(false);
        let (branch, first_read, second_read) =
            (use_second.clone(), Rc::clone(&first), Rc::clone(&second));
        let effect = Effect::new(move || {
            if branch.get() {
                second_read.track();
            } else {
                first_read.track();
            }
        });
        use_second.set(true);
        let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            flush_signal_dirty(&mut first_world);
        }));
        assert!(failure.is_err());
        drop(effect);
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
    fn computed_first_registered_model_branch_switch_does_not_allocate() {
        reset();
        let mut world = World::new();
        drop(OwnerGuard::enter(&mut world));
        let first = Rc::new(ModelSource::new());
        let second = Rc::new(ModelSource::new());
        first.register_owner(world.id());
        second.register_owner(world.id());
        let use_first = Signal::new(true);
        let evaluations = Rc::new(Cell::new(0u32));
        let (branch, first_read, second_read, visits) = (
            use_first.clone(),
            Rc::clone(&first),
            Rc::clone(&second),
            Rc::clone(&evaluations),
        );
        let computed = with_world_scope(&mut world, || {
            Computed::new(move || {
                visits.set(visits.get() + 1);
                if branch.get() {
                    first_read.track();
                    1u8
                } else {
                    second_read.track();
                    2u8
                }
            })
        });
        assert_eq!(computed.get(), 1);
        assert_eq!(evaluations.get(), 1);

        let allocations = graph::tests::tracked_allocations(|| {
            use_first.set(false);
            assert_eq!(computed.get(), 2);
        });
        assert_eq!(allocations, 0);
        assert_eq!(evaluations.get(), 2);
        first.notify();
        assert_eq!(computed.get(), 2);
        assert_eq!(evaluations.get(), 2);
        second.notify();
        assert_eq!(computed.get(), 2);
        assert_eq!(evaluations.get(), 3);
    }

    #[test]
    fn model_source_notification_dirties_computed_before_lazy_evaluation() {
        reset();
        let mut world = World::new();
        drop(OwnerGuard::enter(&mut world));
        let source = Rc::new(ModelSource::new());
        source.register_owner(world.id());
        let evaluations = Rc::new(Cell::new(0u32));
        let (observed, visits) = (Rc::clone(&source), Rc::clone(&evaluations));
        let computed = with_world_scope(&mut world, || {
            Computed::new(move || {
                observed.track();
                visits.set(visits.get() + 1);
                1u8
            })
        });
        let read = computed.clone();
        let effect = with_world_scope(&mut world, || {
            Effect::new(move || {
                let _ = read.get();
            })
        });
        assert_eq!(evaluations.get(), 1);

        source.notify();
        assert!(computed.inner.borrow().dirty);
        assert_eq!(
            evaluations.get(),
            1,
            "notification must not evaluate user code"
        );
        assert!(with_reactive(|r| r.dirty_effects.contains(&effect.id)));
        flush_signal_dirty(&mut world);
        assert_eq!(evaluations.get(), 2);
    }

    #[test]
    fn dropping_model_source_invalidates_cached_computed_before_removing_edges() {
        reset();
        let mut world = World::new();
        let source = Rc::new(ModelSource::new());
        drop(OwnerGuard::enter(&mut world));
        source.register_owner(world.id());
        let weak = Rc::downgrade(&source);
        let computed = with_world_scope(&mut world, || {
            Computed::new(move || {
                weak.upgrade().expect("model source is gone").track();
                9u8
            })
        });
        assert_eq!(computed.get(), 9);

        drop(source);
        let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| computed.get()));
        assert!(
            failure.is_err(),
            "removed source cannot leave a valid cache"
        );
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
    fn owned_widget_scope_switches_signal_edges_without_allocating() {
        reset();
        let mut world = World::new();
        let widget = world.spawn_empty();
        let choose_right = Signal::new(false);
        let left = Signal::new(1u8);
        let right = Signal::new(10u8);
        with_world_scope(&mut world, || {
            with_scope(Subscriber::Widget(widget), || {
                if choose_right.get() {
                    let _ = right.get();
                } else {
                    let _ = left.get();
                }
            });
        });
        assert!(left.inner.borrow().subscribers.is_empty());

        choose_right.set(true);
        flush_signal_dirty(&mut world);
        world.remove::<Dirty>(widget);
        let allocations = graph::tests::tracked_allocations(|| {
            with_world_scope(&mut world, || {
                with_scope(Subscriber::Widget(widget), || {
                    if choose_right.get() {
                        let _ = right.get();
                    } else {
                        let _ = left.get();
                    }
                });
            });
        });
        assert_eq!(allocations, 0, "reevaluation must reuse graph storage");

        left.set(2);
        flush_signal_dirty(&mut world);
        assert!(!world.has::<Dirty>(widget), "old edge must be removed");
        assert_eq!(graph::tests::tracked_allocations(|| right.set(11)), 0);
        flush_signal_dirty(&mut world);
        assert!(world.has::<Dirty>(widget));

        cleanup_effects_for(&world, widget);
        world.remove::<Dirty>(widget);
        right.set(12);
        flush_signal_dirty(&mut world);
        assert!(
            !world.has::<Dirty>(widget),
            "teardown removes graph consumer"
        );
    }

    #[test]
    fn owned_visual_scope_isolates_model_sources_across_worlds() {
        reset();
        let mut first_world = World::new();
        let mut second_world = World::new();
        let first_widget = first_world.spawn_empty();
        let second_widget = second_world.spawn_empty();
        let first_owner = first_world.id();
        let second_owner = second_world.id();
        assert_eq!(first_widget, second_widget);
        let first = ModelSource::new();
        let second = ModelSource::new();
        with_world_scope(&mut first_world, || {
            first.register_owner(first_owner);
            with_scope(Subscriber::VisualWidget(first_widget), || first.track());
        });
        with_world_scope(&mut second_world, || {
            second.register_owner(second_owner);
            with_scope(Subscriber::VisualWidget(second_widget), || second.track());
        });
        assert!(first.subscribers.borrow().is_empty());
        assert!(second.subscribers.borrow().is_empty());

        assert_eq!(graph::tests::tracked_allocations(|| first.notify()), 0);
        flush_signal_dirty(&mut second_world);
        assert!(!second_world.has::<VisualDirty>(second_widget));
        flush_signal_dirty(&mut first_world);
        assert!(first_world.has::<VisualDirty>(first_widget));

        first_world.remove::<VisualDirty>(first_widget);
        let backup = ModelSource::new();
        backup.register_owner(first_owner);
        let allocations = graph::tests::tracked_allocations(|| {
            with_world_scope(&mut first_world, || {
                with_scope(Subscriber::VisualWidget(first_widget), || backup.track());
            });
        });
        assert_eq!(
            allocations, 0,
            "visual dependency switch must reuse storage"
        );
        first.notify();
        flush_signal_dirty(&mut first_world);
        assert!(!first_world.has::<VisualDirty>(first_widget));
        backup.notify();
        flush_signal_dirty(&mut first_world);
        assert!(first_world.has::<VisualDirty>(first_widget));

        let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            with_world_scope(&mut second_world, || {
                with_scope(Subscriber::VisualWidget(second_widget), || first.track());
            });
        }));
        assert!(failure.is_err(), "cross-World model edge must be rejected");
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
        let source = signal.inner.borrow().graph_source;
        let mut subscribers = 0;
        with_reactive(|r| {
            r.graph
                .for_each_subscriber(source, |_, subscriber| {
                    assert!(matches!(subscriber, Subscriber::Effect(_)));
                    subscribers += 1;
                })
                .unwrap();
        });
        assert_eq!(subscribers, 1);
    }

    #[test]
    fn dropped_world_releases_owner_graph_and_effect_edges() {
        reset();
        let mut world = World::new();
        let world_id = world.id();
        let signal = Signal::new(0u8);
        let source = signal.clone();
        let runs = Rc::new(Cell::new(0u32));
        let counter = Rc::clone(&runs);
        let effect = with_world_scope(&mut world, || {
            Effect::new(move || {
                let _ = source.get();
                counter.set(counter.get() + 1);
            })
        });
        assert_eq!(runs.get(), 1);
        assert!(with_reactive(|r| r.graph.owner_key(world_id).is_ok()));

        drop(world);
        assert!(with_reactive(|r| r.graph.owner_key(world_id).is_err()));
        assert_eq!(effect_count(), 0);
        let mut next_world = World::new();
        with_world_scope(&mut next_world, || ());
        signal.set(1);
        assert_eq!(runs.get(), 1);
        drop(effect);
    }
}
