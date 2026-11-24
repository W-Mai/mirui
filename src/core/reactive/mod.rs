extern crate alloc;

mod effect_index;
mod graph;
mod identity;
pub(crate) mod retained;

use alloc::collections::{BTreeMap, VecDeque};
use alloc::rc::{Rc, Weak};
use alloc::vec::Vec;
use core::cell::{Cell, RefCell};
use core::num::NonZeroU64;
use core::ptr::NonNull;

use crate::ecs::world::WorldId;
use crate::ecs::{Entity, World};
use crate::ui::dirty::{Dirty, VisualDirty};
use effect_index::EffectIndex;
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

struct PendingEffect {
    id: EffectId,
    order: u64,
}

impl PendingEffect {
    fn enqueue(queue: &mut VecDeque<Self>, id: EffectId, effect: &EffectInner) {
        if !effect.queued.get() {
            queue.push_back(Self { id, order: 0 });
            effect.queued.set(true);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct ComputedId(SlotId);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(crate) struct SourceId(SlotId);

#[derive(Clone, Copy)]
struct WorldWindow {
    pointer: NonNull<World>,
    owner: WorldId,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WorldAccess {
    Borrowed(WorldId),
    Reborrowed(WorldId),
    Suppressed(WorldId),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PaintRequestCondition {
    Always,
    OnChange,
    OnResourceWake,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct VisualInvalidation {
    owner: Option<WorldId>,
    entity: Entity,
    pending: Option<NonZeroU64>,
    condition: PaintRequestCondition,
}

impl VisualInvalidation {
    fn ready(owner: Option<WorldId>, entity: Entity) -> Self {
        Self {
            owner,
            entity,
            pending: None,
            condition: PaintRequestCondition::Always,
        }
    }
}

struct Reactive {
    scope: Option<Subscriber>,
    scope_graph_consumer: Option<ConsumerKey>,
    // The structural window belongs to WorldGuard, not to owner-only scopes.
    world_window: Option<WorldWindow>,
    world_id: Option<WorldId>,
    world_lifetime: Option<Weak<()>>,
    borrowed_worlds: [Option<WorldAccess>; 8],
    model_read_only_depth: u16,
    dirty_widgets: VecDeque<(Option<WorldId>, Entity)>,
    dirty_visual_widgets: VecDeque<VisualInvalidation>,
    next_paint_scope: u64,
    model_visual_subscriptions: usize,
    dirty_effects: VecDeque<PendingEffect>,
    effects: BTreeMap<EffectId, Rc<RefCell<EffectInner>>>,
    unbound_effects: usize,
    effect_index: EffectIndex,
    captured_effects: Option<Vec<EffectId>>,
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
            world_window: None,
            world_id: None,
            world_lifetime: None,
            borrowed_worlds: [None; 8],
            model_read_only_depth: 0,
            dirty_widgets: VecDeque::new(),
            dirty_visual_widgets: VecDeque::new(),
            next_paint_scope: 1,
            model_visual_subscriptions: 0,
            dirty_effects: VecDeque::new(),
            effects: BTreeMap::new(),
            unbound_effects: 0,
            effect_index: EffectIndex::new(),
            captured_effects: None,
            computeds: BTreeMap::new(),
            effect_slots: SlotAllocator::new(),
            computed_slots: SlotAllocator::new(),
            source_slots: SlotAllocator::new(),
            graph: ReactiveGraph::new(),
        }
    }

    fn unbound_effect_count(&self) -> usize {
        self.unbound_effects
    }

    fn reserve_owned_consumer_slot(&mut self, partition: PartitionKey) {
        let headroom = self.unbound_effect_count();
        self.graph
            .reserve_owner_consumer_for(partition, headroom)
            .expect("reactive owner consumer capacity exhausted");
    }
}

fn reserve_queue_total<T>(queue: &mut VecDeque<T>, total: usize) {
    queue.reserve(total.saturating_sub(queue.len()));
}

// Ambient runtime: `Signal::get()` is parameterless, so it finds the current
// consumer here rather than via a passed context. Storage is sealed in this fn
// — std uses a per-thread cell (tests isolate, no lock), no_std a critical_section.
#[cfg(feature = "std")]
std::thread_local! {
    #[cfg_attr(target_os = "android", expect(clippy::missing_const_for_thread_local, reason = "Rust 1.97 Android TLS expansion flags an explicit const initializer"))]
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
                    r.reserve_owned_consumer_slot(partition);
                    match scope {
                        Subscriber::Widget(_) => {
                            let count = r.graph.count_consumers_matching(|target| {
                                matches!(target, Subscriber::Widget(_))
                            });
                            reserve_queue_total(&mut r.dirty_widgets, count + 1);
                        }
                        Subscriber::VisualWidget(_) => {
                            let count = r.graph.count_consumers_matching(|target| {
                                matches!(target, Subscriber::VisualWidget(_))
                            });
                            reserve_queue_total(
                                &mut r.dirty_visual_widgets,
                                count + r.model_visual_subscriptions + 1,
                            );
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
        let ready = VisualInvalidation::ready(owner, entity);
        if !r.dirty_visual_widgets.contains(&ready) {
            r.dirty_visual_widgets.push_back(ready);
        }
    });
}

/// Prepare additional request slots before paint callbacks run. Existing work, including other Worlds' notifications, remains resident.
pub(crate) fn reserve_paint_requests(additional: usize) -> bool {
    with_reactive(|r| {
        r.dirty_visual_widgets
            .try_reserve(additional.saturating_mul(2))
            .is_ok()
    })
}

/// Resource completion only sets readiness; entity invalidation stays on the owning thread.
pub(crate) struct PaintWake(crate::core::resource::wake::TaskWake);

impl PaintWake {
    pub(crate) fn new() -> Self {
        let wake = crate::core::resource::wake::TaskWake::new();
        wake.take_ready();
        Self(wake)
    }

    pub(crate) fn register(&self, parent: &core::task::Waker) {
        self.0.register(parent);
    }
}

/// Pending visual invalidations owned by one independently successful paint operation.
pub(crate) struct PaintRequestScope {
    owner: WorldId,
    token: Option<NonZeroU64>,
    wake: Option<Rc<PaintWake>>,
}

impl PaintRequestScope {
    pub(crate) fn new(world: &World) -> Self {
        world.register_drop_hook::<ReactiveOwnerCleanup>(release_graph_owner);
        let token = with_reactive(|r| {
            let token = NonZeroU64::new(r.next_paint_scope).expect("nonzero paint scope identity");
            r.next_paint_scope = r
                .next_paint_scope
                .checked_add(1)
                .expect("paint scope identity exhausted");
            token
        });
        Self {
            owner: world.id(),
            token: Some(token),
            wake: world.resource::<Rc<PaintWake>>().cloned(),
        }
    }

    pub(crate) fn waker(&self) -> &core::task::Waker {
        self.wake
            .as_ref()
            .map_or(core::task::Waker::noop(), |wake| wake.0.waker())
    }

    /// Waiting survives both fallback publication and a failed strict paint attempt.
    pub(crate) fn wait_for_resource(&self, entity: Entity) -> bool {
        if self.wake.is_none() {
            return false;
        }
        let pending = VisualInvalidation {
            owner: Some(self.owner),
            entity,
            pending: None,
            condition: PaintRequestCondition::OnResourceWake,
        };
        with_reactive(|r| {
            if r.dirty_visual_widgets.contains(&pending) {
                return true;
            }
            if r.dirty_visual_widgets.len() == r.dirty_visual_widgets.capacity() {
                return false;
            }
            r.dirty_visual_widgets.push_back(pending);
            true
        })
    }

    /// Record a request without growing the queue. A committed notification for the same entity remains independent until this operation commits.
    pub(crate) fn request(&self, entity: Entity) -> bool {
        self.request_with_condition(entity, PaintRequestCondition::Always)
    }

    /// Record a request that is committed only when the completed output changed.
    pub(crate) fn request_on_change(&self, entity: Entity) -> bool {
        self.request_with_condition(entity, PaintRequestCondition::OnChange)
    }

    fn request_with_condition(&self, entity: Entity, condition: PaintRequestCondition) -> bool {
        let pending = VisualInvalidation {
            owner: Some(self.owner),
            entity,
            pending: self.token,
            condition,
        };
        with_reactive(|r| {
            if let Some(existing) = r.dirty_visual_widgets.iter_mut().find(|entry| {
                entry.owner == pending.owner
                    && entry.entity == pending.entity
                    && entry.pending == pending.pending
            }) {
                if condition == PaintRequestCondition::Always {
                    existing.condition = PaintRequestCondition::Always;
                }
                return true;
            }
            if r.dirty_visual_widgets.len() == r.dirty_visual_widgets.capacity() {
                return false;
            }
            r.dirty_visual_widgets.push_back(pending);
            true
        })
    }

    pub(crate) fn has_change_requests(&self) -> bool {
        with_reactive(|r| {
            r.dirty_visual_widgets.iter().any(|entry| {
                entry.owner == Some(self.owner)
                    && entry.pending == self.token
                    && entry.condition == PaintRequestCondition::OnChange
            })
        })
    }

    pub(crate) fn commit(self) {
        self.commit_output(false);
    }

    pub(crate) fn commit_output(mut self, changed: bool) {
        let token = self.token.take();
        with_reactive(|r| {
            let queue = &mut r.dirty_visual_widgets;
            let mut index = 0;
            while let Some(entry) = queue.get(index).copied() {
                if entry.owner != Some(self.owner) || entry.pending != token {
                    index += 1;
                    continue;
                }
                if entry.condition == PaintRequestCondition::OnChange && !changed {
                    queue.remove(index);
                    continue;
                }
                let ready = VisualInvalidation::ready(entry.owner, entry.entity);
                if queue.contains(&ready) {
                    queue.remove(index);
                } else {
                    queue[index] = ready;
                    index += 1;
                }
            }
        });
    }
}

impl Drop for PaintRequestScope {
    fn drop(&mut self) {
        let Some(token) = self.token else { return };
        try_with_reactive(|r| {
            r.dirty_visual_widgets
                .retain(|entry| entry.owner != Some(self.owner) || entry.pending != Some(token));
        });
    }
}

fn enqueue_effect(id: EffectId) {
    with_reactive(|r| {
        if let Some(effect) = r.effects.get(&id) {
            PendingEffect::enqueue(&mut r.dirty_effects, id, &effect.borrow());
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
                            if effect.owner_world.is_none() {
                                r.effect_index.bind_world(world, id);
                            }
                            effect.model_consumer = Some(consumer);
                            if effect.owner_world.replace(world).is_none() {
                                r.unbound_effects -= 1;
                            }
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
                    if let Some(inner) = r.effects.get(&effect) {
                        PendingEffect::enqueue(&mut r.dirty_effects, effect, &inner.borrow());
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
                    let ready = VisualInvalidation::ready(owner, entity);
                    if !r.dirty_visual_widgets.contains(&ready) {
                        r.dirty_visual_widgets.push_back(ready);
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
        let effect_ids = EffectIndex::prepare_world(r.unbound_effect_count())
            .expect("reactive Effect owner index capacity exhausted");
        r.graph
            .reserve_owner_consumer_headroom(r.unbound_effect_count())
            .expect("reactive owner Effect headroom exhausted");
        r.graph
            .reserve_for_owner()
            .expect("reactive owner graph capacity exhausted");
        r.graph
            .add_owner(world)
            .expect("reactive owner registration failed");
        r.effect_index.register_world(world, effect_ids);
    });
}

#[cfg(test)]
pub(crate) fn has_graph_owner(world: WorldId) -> bool {
    with_reactive(|r| r.graph.owner_key(world).is_ok())
}

/// Release graph state and widget-bound effects for a finished World.
pub(crate) fn release_graph_owner(world: WorldId) {
    try_with_reactive(|r| r.effect_index.begin_release(world));
    loop {
        let effect = try_with_reactive(|r| r.effect_index.first_world(world)).flatten();
        let Some(effect) = effect else { break };
        unregister_effect(effect);
    }
    try_with_reactive(|r| {
        r.effect_index.remove_world(world);
        r.graph.remove_owner(world);
        r.dirty_widgets.retain(|(owner, _)| *owner != Some(world));
        r.dirty_visual_widgets
            .retain(|entry| entry.owner != Some(world));
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
    prev: Option<WorldWindow>,
    reborrowed: Option<usize>,
    _owner: OwnerGuard,
}

impl WorldGuard {
    pub(crate) fn enter(world: &mut World) -> Self {
        let owner = OwnerGuard::enter(world);
        let window = WorldWindow {
            owner: world.id(),
            pointer: NonNull::from(world),
        };
        let (prev, reborrowed) = with_reactive(|r| {
            // The explicit exclusive reference suspends its outer ambient borrow.
            // Keep that slot occupied until this narrower window ends.
            let reborrowed = r
                .borrowed_worlds
                .iter()
                .position(|access| *access == Some(WorldAccess::Borrowed(window.owner)));
            if let Some(slot) = reborrowed {
                r.borrowed_worlds[slot] = Some(WorldAccess::Reborrowed(window.owner));
            }
            (r.world_window.replace(window), reborrowed)
        });
        Self {
            prev,
            reborrowed,
            _owner: owner,
        }
    }
}

pub(crate) struct WorldAccessGuard {
    slot: usize,
    _thread: ::core::marker::PhantomData<Rc<()>>,
}

impl WorldAccessGuard {
    fn borrow(id: WorldId) -> Self {
        Self::enter(WorldAccess::Borrowed(id))
    }

    pub(crate) fn suppress(world: &World) -> Self {
        Self::enter(WorldAccess::Suppressed(world.id()))
    }

    fn enter(access: WorldAccess) -> Self {
        let slot = with_reactive(|r| {
            if let WorldAccess::Borrowed(id) = access {
                assert!(
                    !r.borrowed_worlds.contains(&Some(WorldAccess::Borrowed(id))),
                    "World is already mutably borrowed through with_world"
                );
            }
            let slot = r
                .borrowed_worlds
                .iter()
                .position(Option::is_none)
                .expect("too many nested World borrows");
            r.borrowed_worlds[slot] = Some(access);
            slot
        });
        Self {
            slot,
            _thread: ::core::marker::PhantomData,
        }
    }
}

impl Drop for WorldAccessGuard {
    fn drop(&mut self) {
        with_reactive(|r| r.borrowed_worlds[self.slot] = None);
    }
}

impl Drop for WorldGuard {
    fn drop(&mut self) {
        with_reactive(|r| {
            r.world_window = self.prev;
            if let Some(slot) = self.reborrowed
                && let Some(WorldAccess::Reborrowed(owner)) = r.borrowed_worlds[slot]
            {
                r.borrowed_worlds[slot] = Some(WorldAccess::Borrowed(owner));
            }
        });
    }
}

// An ownerless Effect cannot safely use whichever World happened to flush
// first. Hide that ambient World until the Effect reads a model and binds to
// the model's owner; no user closure runs under the runtime borrow.
struct WorldlessEffectGuard {
    previous_world: Option<WorldWindow>,
    previous_owner: Option<WorldId>,
    previous_lifetime: Option<Weak<()>>,
}

impl WorldlessEffectGuard {
    fn enter() -> Self {
        let (previous_world, previous_owner, previous_lifetime) = with_reactive(|r| {
            (
                r.world_window.take(),
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
            r.world_window = self.previous_world;
            r.world_id = self.previous_owner;
            r.world_lifetime = self.previous_lifetime.take();
        });
    }
}

/// Make the World reachable by effect closures while `f` runs, including reactive bindings initialized during construction.
pub(crate) fn with_world_scope<R>(world: &mut World, f: impl FnOnce() -> R) -> R {
    let _guard = WorldGuard::enter(world);
    f()
}

/// Reach the structural World window associated with the current reactive owner; return None when no matching window exists. An owner-only scope for another World cannot inherit that window.
///
/// An ownerless Effect returns None even when a World flushes it. The run that binds it to a model also remains ownerless; later runs in the model's World can access that World.
pub(crate) fn with_world<R>(f: impl FnOnce(&mut World) -> R) -> Option<R> {
    // Copy the window before deref so `f` may re-enter with_reactive.
    let window = with_reactive(|r| {
        let window = r
            .world_window
            .filter(|window| Some(window.owner) == r.world_id)?;
        (!r.borrowed_worlds
            .contains(&Some(WorldAccess::Suppressed(window.owner))))
        .then_some(window)
    })?;
    let _borrow = WorldAccessGuard::borrow(window.owner);
    // SAFETY: a window exists only within flush_signal_dirty / with_world_scope, which
    // hold a live &mut World; single-threaded. Windows may nest (a reactive
    // walk/if/match re-run enters another scope); WorldGuard saves/restores the
    // previous pointer LIFO, so the innermost live &mut World always wins.
    Some(f(unsafe { &mut *window.pointer.as_ptr() }))
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

fn pop_visual_widget_for(
    queue: &mut VecDeque<VisualInvalidation>,
    world_id: WorldId,
) -> Option<Entity> {
    let entry = queue.pop_front().expect("queued visual widget");
    if entry.pending.is_none()
        && entry.condition != PaintRequestCondition::OnResourceWake
        && entry.owner.is_none_or(|owner| owner == world_id)
    {
        Some(entry.entity)
    } else {
        queue.push_back(entry);
        None
    }
}

fn effect_tree_revision(world: &World) -> (u64, u64) {
    (
        world.component_revision::<crate::ui::Parent>(),
        world.component_revision::<crate::ui::Children>(),
    )
}

fn order_effects(world: &World, remaining: usize) {
    if remaining < 2 {
        return;
    }
    let retained = world.component_count(core::any::TypeId::of::<retained::RetainedEffects>()) > 0;
    if !retained
        && world.component_count(core::any::TypeId::of::<
            crate::ui::branch::CachedBranchVisibility,
        >()) == 0
    {
        return;
    }
    #[cfg(feature = "perf")]
    crate::trace_span!("reactive.order");
    let ancestry_depth = retained::ancestry_depth(world);
    let world_id = world.id();
    with_reactive(|runtime| {
        let count = remaining.min(runtime.dirty_effects.len());
        for (index, entry) in runtime.dirty_effects.iter_mut().take(count).enumerate() {
            entry.order = runtime.effects.get(&entry.id).map_or(0, |effect| {
                let effect = effect.borrow();
                let foreign = effect.owner_world.is_some_and(|owner| owner != world_id);
                let ordinary = effect.role == BindingRole::Ordinary;
                let (lane, depth) = if foreign {
                    // Entity identities only resolve within their own World.
                    (3, 0)
                } else if ordinary && !retained {
                    (1, index)
                } else {
                    (
                        if ordinary { 1 } else { 2 },
                        effect.owner_entity.map_or(0, &ancestry_depth),
                    )
                };
                // The 32-bit identity space bounds both depth and the queue ordinal.
                // Keep the cycle sentinel above every finite depth on all targets.
                let depth = if depth == usize::MAX {
                    1u64 << 32
                } else {
                    depth as u64
                };
                // Zero sorts missing effects first. Each live lane has 33 depth
                // bits and two role bits; the full ID breaks remaining ties.
                (lane << 35) | (depth << 2) | u64::from(effect.role as u8)
            });
        }
        runtime.dirty_effects.make_contiguous()[..count]
            .sort_unstable_by_key(|entry| (entry.order, entry.id));
    });
}

/// Drain queued reactive work at input delivery and frame boundaries:
/// re-run dirty effects (which may dirty more), then mark dirty widgets. Dead
/// entities are skipped (no reverse index; subscriber lists may retain
/// despawned entities).
#[cfg_attr(feature = "perf", crate::trace_fn("reactive.flush"))]
pub fn flush_signal_dirty(world: &mut World) {
    let resource_ready = world
        .resource::<Rc<PaintWake>>()
        .is_some_and(|wake| wake.0.take_ready());
    with_reactive(|r| {
        if !r.dirty_visual_widgets.is_empty() {
            r.dirty_visual_widgets.retain_mut(|entry| {
                if entry.owner == Some(world.id())
                    && entry.condition == PaintRequestCondition::OnResourceWake
                {
                    if !world.is_alive(entry.entity) {
                        return false;
                    }
                    if resource_ready {
                        entry.condition = PaintRequestCondition::Always;
                    }
                }
                true
            });
        }
    });
    // Preserve cold preparation even when no callback needs a structural window.
    world.register_drop_hook::<ReactiveOwnerCleanup>(release_graph_owner);
    register_graph_owner(world.id());
    reclaim_dead_effects(world);
    // Retiring captures may enqueue work, so inspect the queues after cleanup.
    if with_reactive(|r| {
        r.dirty_effects.is_empty()
            && r.dirty_widgets.is_empty()
            && r.dirty_visual_widgets.is_empty()
    }) {
        return;
    }
    let _guard = WorldGuard::enter(world);
    let world_id = world.id();
    for _ in 0..FLUSH_MAX_PASSES {
        let mut ran_effect = false;
        {
            #[cfg(feature = "perf")]
            crate::trace_span!("reactive.effects");
            let pending_effects = with_reactive(|r| r.dirty_effects.len());
            if pending_effects > 1 {
                order_effects(world, pending_effects);
            }
            let mut tree_revision = effect_tree_revision(world);
            for index in 0..pending_effects {
                let revision = effect_tree_revision(world);
                if revision != tree_revision {
                    order_effects(world, pending_effects - index);
                    tree_revision = revision;
                }
                let effect = with_reactive(|r| {
                    let entry = r.dirty_effects.pop_front()?;
                    let id = entry.id;
                    let effect = r.effects.get(&id)?.borrow();
                    if effect.owner_world.is_some_and(|owner| owner != world_id) {
                        r.dirty_effects.push_back(entry);
                        return None;
                    }
                    effect.queued.set(false);
                    Some((id, effect.owner_entity, effect.role))
                });
                if let Some((effect, owner, role)) = effect {
                    if owner.is_some_and(|entity| {
                        !world.is_alive(entity)
                            || !retained::enabled(world, entity)
                            || (role == BindingRole::Display
                                && !retained::display_enabled(world, entity))
                    }) {
                        continue;
                    }
                    run_effect(effect);
                    ran_effect = true;
                }
            }
        }
        let mut marked_widget = false;
        let mut marked_visual_widget = false;
        {
            #[cfg(feature = "perf")]
            crate::trace_span!("reactive.mark_dirty");
            let pending_widgets = with_reactive(|r| r.dirty_widgets.len());
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
            for _ in 0..pending_visual_widgets {
                let entity =
                    with_reactive(|r| pop_visual_widget_for(&mut r.dirty_visual_widgets, world_id));
                if let Some(entity) = entity {
                    marked_visual_widget = true;
                    if world.is_alive(entity) {
                        world.insert(entity, VisualDirty::New);
                    }
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
#[cfg_attr(feature = "perf", crate::trace_fn("reactive.reclaim"))]
fn reclaim_dead_effects(world: &mut World) {
    reclaim_entity_consumers(world);
    // Rc lifetimes are independent of entity retirement and graph registration.
    with_reactive(|r| {
        if !r.computeds.is_empty() {
            let slots = &mut r.computed_slots;
            r.computeds.retain(|id, weak| {
                if weak.strong_count() > 0 {
                    true
                } else {
                    slots.release(id.0);
                    false
                }
            });
        }
    });
}

fn reclaim_entity_consumers(world: &mut World) {
    let Some(stamp) = with_reactive(|r| {
        r.graph
            .pending_reclamation(world.id(), world.retirement_revision())
    }) else {
        return;
    };
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
    if !dead.is_empty() {
        // Captured values may use the structural World window while dropping.
        let _guard = WorldGuard::enter(world);
        for id in dead {
            unregister_effect(id);
        }
    }
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
    // Drops can register consumers or retire entities; never acknowledge those
    // changes using a stamp collected after the scan.
    with_reactive(|r| r.graph.acknowledge_reclamation(world.id(), stamp));
}

/// Reclaim effects bound to `entity` in `world` during widget teardown.
pub(crate) fn cleanup_effects_for(world: &World, entity: Entity) {
    loop {
        let bound = with_reactive(|r| {
            r.effect_index.first_entity(world.id(), entity, |id| {
                r.effects.get(&id).is_some_and(|effect| {
                    let effect = effect.borrow();
                    effect.owner_world == Some(world.id()) && effect.owner_entity == Some(entity)
                })
            })
        });
        let Some(id) = bound else { break };
        unregister_effect(id);
    }
    with_reactive(|r| {
        let Ok(partition) = r.graph.owner_key(world.id()) else {
            return;
        };
        for consumer in r
            .graph
            .find_widget_consumers(partition, entity)
            .into_iter()
            .flatten()
        {
            r.graph
                .remove_consumer(consumer)
                .expect("Widget graph consumer missing during teardown");
            if r.scope_graph_consumer == Some(consumer) {
                r.scope_graph_consumer = None;
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

pub(crate) struct WeakSignal<T: 'static>(Weak<RefCell<SignalInner<T>>>);

impl<T: 'static> WeakSignal<T> {
    pub(crate) fn upgrade(&self) -> Option<Signal<T>> {
        self.0.upgrade().map(|inner| Signal { inner })
    }
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
    pub(crate) fn downgrade(&self) -> WeakSignal<T> {
        WeakSignal(Rc::downgrade(&self.inner))
    }

    pub(crate) fn has_subscribers(&self) -> bool {
        let inner = self.inner.borrow();
        if !inner.subscribers.is_empty() {
            return true;
        }
        with_reactive(|r| {
            let mut observed = false;
            r.graph
                .for_each_subscriber(inner.graph_source, |_, _| observed = true)
                .expect("live Signal source");
            observed
        })
    }

    pub(crate) fn observe_view(
        &self,
        world: WorldId,
        entity: Entity,
        visual: bool,
    ) -> ViewSubscription {
        let source = self.inner.borrow().graph_source;
        ViewSubscription::new(
            source,
            world,
            if visual {
                Subscriber::VisualWidget(entity)
            } else {
                Subscriber::Widget(entity)
            },
        )
    }

    #[cfg(feature = "persistence")]
    pub(crate) fn source_id(&self) -> SourceId {
        self.inner.borrow().id
    }

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
    lifecycle: Cell<ModelSourceLifecycle>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ModelSourceLifecycle {
    Active,
    Retired,
    Exhausted,
}

/// One graph consumer owned by a View observation binding.
pub(crate) struct ViewSubscription {
    consumer: ConsumerKey,
}

impl ViewSubscription {
    fn new(source: SourceKey, world: WorldId, target: Subscriber) -> Self {
        let consumer = with_reactive(|r| {
            let partition = r
                .graph
                .owner_key(world)
                .expect("View owner is not registered");
            r.reserve_owned_consumer_slot(partition);
            match target {
                Subscriber::Widget(_) => {
                    let count = r
                        .graph
                        .count_consumers_matching(|target| matches!(target, Subscriber::Widget(_)));
                    reserve_queue_total(&mut r.dirty_widgets, count + 1);
                }
                Subscriber::VisualWidget(_) => {
                    let count = r.graph.count_consumers_matching(|target| {
                        matches!(target, Subscriber::VisualWidget(_))
                    });
                    reserve_queue_total(
                        &mut r.dirty_visual_widgets,
                        r.model_visual_subscriptions + count + 1,
                    );
                }
                Subscriber::Effect(_) | Subscriber::Computed(_) => {
                    unreachable!("View subscriptions target widgets")
                }
            }
            let consumer = r
                .graph
                .register_explicit_consumer(partition, target)
                .expect("reactive View consumer registration failed");
            if let Err(error) = r.graph.subscribe(source, consumer) {
                let _ = r.graph.remove_consumer(consumer);
                panic!("reactive View dependency subscription failed: {error:?}");
            }
            consumer
        });
        Self { consumer }
    }
}

impl Drop for ViewSubscription {
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
            lifecycle: Cell::new(ModelSourceLifecycle::Active),
        }
    }

    pub(crate) fn register_owner(&self, owner: WorldId) {
        assert!(
            self.lifecycle.get() != ModelSourceLifecycle::Exhausted,
            "model source identity is exhausted"
        );
        if let Some(source) = self.graph_source.get() {
            assert!(
                matches!(source.partition, PartitionKey::Owner { world, .. } if world == owner),
                "model source is already registered to another App"
            );
            self.lifecycle.set(ModelSourceLifecycle::Active);
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
        self.lifecycle.set(ModelSourceLifecycle::Active);
    }

    pub(crate) fn can_reactivate(&self, owner: WorldId) -> bool {
        self.lifecycle.get() != ModelSourceLifecycle::Exhausted
            && self.graph_source.get().is_some_and(|source| {
                matches!(source.partition, PartitionKey::Owner { world, .. } if world == owner)
            })
    }

    pub(crate) fn can_rebind(&self, owner: WorldId) -> bool {
        self.can_reactivate(owner)
            && (self.lifecycle.get() != ModelSourceLifecycle::Active
                || self
                    .graph_source
                    .get()
                    .is_some_and(|source| source.slot.generation < u32::MAX))
    }

    pub fn track(&self) {
        if self.lifecycle.get() != ModelSourceLifecycle::Active {
            return;
        }
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
                    reserve_queue_total(&mut r.dirty_visual_widgets, next + graph_visuals);
                    r.model_visual_subscriptions = next;
                });
            }
            subscribers.push(subscriber);
        }
    }

    fn subscribe_view(&self, world: WorldId, target: Subscriber) -> ViewSubscription {
        assert!(
            self.lifecycle.get() == ModelSourceLifecycle::Active,
            "model source is retired"
        );
        let source = self
            .graph_source
            .get()
            .expect("View model source must be registered");
        with_reactive(|r| {
            let partition = r
                .graph
                .owner_key(world)
                .expect("View owner is not registered");
            assert_eq!(
                source.partition, partition,
                "model belongs to a different App"
            );
        });
        ViewSubscription::new(source, world, target)
    }

    pub(crate) fn subscribe_widget(&self, world: WorldId, entity: Entity) -> ViewSubscription {
        self.subscribe_view(world, Subscriber::Widget(entity))
    }

    pub(crate) fn subscribe_visual_widget(
        &self,
        world: WorldId,
        entity: Entity,
    ) -> ViewSubscription {
        self.subscribe_view(world, Subscriber::VisualWidget(entity))
    }

    pub fn notify(&self) {
        if self.lifecycle.get() != ModelSourceLifecycle::Active {
            return;
        }
        if let Some(source) = self.graph_source.get() {
            publish_graph_source(source);
        }
        let len = self.subscribers.borrow().len();
        for index in 0..len {
            let subscriber = self.subscribers.borrow()[index];
            propagate(subscriber);
        }
    }

    fn invalidate_computeds(&self) {
        if let Some(source) = self.graph_source.get() {
            invalidate_computed_source(source);
        }
    }
    pub(crate) fn retire(&self) {
        if self.lifecycle.get() != ModelSourceLifecycle::Active {
            return;
        }
        self.lifecycle.set(ModelSourceLifecycle::Retired);
        self.invalidate_computeds();
        let source = self.graph_source.get();
        let remaining_visuals = {
            let mut subscribers = self.subscribers.borrow_mut();
            let visuals = subscribers
                .iter()
                .filter(|entry| matches!(entry.subscriber, Subscriber::VisualWidget(_)))
                .count();
            subscribers.clear();
            visuals
        };
        if remaining_visuals != 0 || source.is_some() {
            try_with_reactive(|r| {
                if let Some(source) = source {
                    match r.graph.renew_owner_source(source) {
                        Ok(renewed) => self.graph_source.set(Some(renewed)),
                        Err(graph::GraphError::IdentityExhausted) => {
                            self.lifecycle.set(ModelSourceLifecycle::Exhausted)
                        }
                        Err(graph::GraphError::StaleSource) => {}
                        Err(error) => panic!("model source retirement failed: {error:?}"),
                    }
                }
                r.model_visual_subscriptions -= remaining_visuals;
            });
        }
    }
}

impl Drop for ModelSource {
    fn drop(&mut self) {
        self.retire();
        if let Some(source) = self.graph_source.take() {
            try_with_reactive(|r| {
                let _ = r.graph.remove_source(source);
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
    role: BindingRole,
    // Tracks the full EffectId's membership in dirty_effects, including foreign entries.
    queued: Cell<bool>,
}

/// Execution responsibility assigned by framework-generated bindings.
#[doc(hidden)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BindingRole {
    Maintenance,
    Ordinary,
    Display,
}

fn unregister_effect(id: EffectId) {
    let removed = try_with_reactive(|r| {
        let removed = r.effects.remove(&id);
        if let Some(effect) = &removed {
            let effect = effect.borrow();
            if effect.owner_world.is_none() {
                r.unbound_effects -= 1;
            } else if let Some(world) = effect.owner_world {
                assert!(
                    r.effect_index.remove(world, effect.owner_entity, id),
                    "Effect owner index missing during cleanup"
                );
            }
            if let Some(consumer) = effect.model_consumer {
                let _ = r.graph.remove_consumer(consumer);
            }
            let _ = r.graph.remove_consumer(effect.graph_consumer);
            if effect.queued.get() {
                r.dirty_effects.retain(|queued| queued.id != id);
            }
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
        Self::spawn(f, None, BindingRole::Ordinary)
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

    fn spawn(f: impl Fn() + 'static, owner_entity: Option<Entity>, role: BindingRole) -> Effect {
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
            if let Some(world) = owner_world {
                r.reserve_owned_consumer_slot(partition);
                r.effect_index
                    .prepare_owned(world, r.unbound_effects)
                    .expect("reactive Effect index capacity exhausted");
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
                r.effect_index
                    .reserve_unbound(r.graph.owner_worlds(), needed)
                    .expect("reactive unbound Effect index capacity exhausted");
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
                role,
                queued: Cell::new(false),
            }));
            r.effects.insert(id, inner);
            if let Some(world) = owner_world {
                r.effect_index.insert_owned(world, owner_entity, id);
            }
            if owner_world.is_none() {
                r.unbound_effects += 1;
            }
            if let Some(captured) = &mut r.captured_effects {
                captured.push(id);
            }
            let effect_count = r.effects.len();
            reserve_queue_total(&mut r.dirty_effects, effect_count);
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

/// Owned effects created while mounting a compound UI member.
#[doc(hidden)]
pub struct OwnedEffects {
    ids: Vec<EffectId>,
    local: core::marker::PhantomData<Rc<()>>,
}

impl Drop for OwnedEffects {
    fn drop(&mut self) {
        for id in self.ids.drain(..) {
            unregister_effect(id);
        }
    }
}

struct EffectCapture {
    previous: Option<Vec<EffectId>>,
}

impl Drop for EffectCapture {
    fn drop(&mut self) {
        let unfinished = with_reactive(|runtime| {
            core::mem::replace(&mut runtime.captured_effects, self.previous.take())
        });
        if let Some(ids) = unfinished {
            drop(OwnedEffects {
                ids,
                local: core::marker::PhantomData,
            });
        }
    }
}

/// Retain newly created effects until their compound member is retired.
#[doc(hidden)]
pub fn with_owned_effects<T>(build: impl FnOnce() -> T) -> (T, OwnedEffects) {
    let capture = EffectCapture {
        previous: with_reactive(|runtime| runtime.captured_effects.replace(Vec::new())),
    };
    let value = build();
    let ids = with_reactive(|runtime| {
        runtime
            .captured_effects
            .take()
            .expect("active effect capture")
    });
    drop(capture);
    (
        value,
        OwnedEffects {
            ids,
            local: core::marker::PhantomData,
        },
    )
}

/// An effect owned by the runtime and tied to a widget entity: it re-runs on
/// dependency change and lives until the widget is despawned (cleanup later),
/// not until a returned handle drops. Hence no handle is returned — dropping
/// one would immediately unregister the effect.
#[cfg_attr(not(test), allow(dead_code))]
pub fn effect_with_widget(entity: Entity, f: impl Fn() + 'static) {
    effect_with_role(entity, BindingRole::Ordinary, f);
}

#[doc(hidden)]
pub fn effect_with_role(entity: Entity, role: BindingRole, f: impl Fn() + 'static) {
    if role != BindingRole::Ordinary {
        with_world(|world| {
            world.watch_component_revision::<crate::ui::Parent>();
            world.watch_component_revision::<crate::ui::Children>();
        });
    }
    core::mem::forget(Effect::spawn(f, Some(entity), role));
}

pub(crate) mod widget_binding {
    use super::{BindingRole, Entity, World, effect_with_role, with_world_scope};

    pub struct PreparedWidgetEffect<F> {
        entity: Entity,
        run: F,
        role: BindingRole,
    }

    pub fn prepare_widget_effect<F: Fn() + 'static>(
        entity: Entity,
        role: BindingRole,
        run: F,
    ) -> PreparedWidgetEffect<F> {
        PreparedWidgetEffect { entity, run, role }
    }

    impl<F: Fn() + 'static> PreparedWidgetEffect<F> {
        pub fn install_once(self, world: &mut World) {
            with_world_scope(world, self.run);
        }

        pub fn install(self, world: &mut World) {
            with_world_scope(world, || effect_with_role(self.entity, self.role, self.run));
        }
    }
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
                r.reserve_owned_consumer_slot(partition);
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
#[path = "../../../tests/unit/core/reactive/binding_tests.rs"]
mod binding_tests;
#[cfg(test)]
#[path = "../../../tests/unit/core/reactive/tests.rs"]
mod tests;
