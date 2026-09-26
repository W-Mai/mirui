//! Registered model instances and their shared method-call boundary.

use alloc::boxed::Box;
use alloc::rc::{Rc, Weak};
use core::cell::{Cell, RefCell};

use crate::ecs::world::WorldId;

/// Derived observations declared by a model's inherent implementation.
#[doc(hidden)]
pub trait ModelMethods: Sized {
    type DerivedSnapshot: Copy;
    type DerivedSources: AsRef<[crate::core::reactive::ModelSource]>;
    type Events;
    type Routes;

    #[doc(hidden)]
    fn derived_snapshot(&self) -> Self::DerivedSnapshot;

    #[doc(hidden)]
    fn derived_sources() -> Self::DerivedSources;

    #[doc(hidden)]
    fn publish_derived(
        sources: &Self::DerivedSources,
        before: Self::DerivedSnapshot,
        after: Self::DerivedSnapshot,
    );

    #[doc(hidden)]
    fn take_events(&mut self) -> Self::Events;

    #[doc(hidden)]
    fn routes() -> Self::Routes;

    #[doc(hidden)]
    fn deliver_events(routes: &Self::Routes, events: Self::Events);
}

/// A model declares that it produces one effect type.
pub trait Produces<E>: ModelMethods {
    #[doc(hidden)]
    fn route(routes: &Self::Routes) -> &EffectRoute<E>;
}

/// Registration failure for an effect consumer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectRegistrationError {
    AlreadyRegistered,
}

type EffectHandler<E> = Box<dyn Fn(E)>;

/// One callback slot for one effect type on one model instance.
#[doc(hidden)]
pub struct EffectRoute<E> {
    handler: RefCell<Option<EffectHandler<E>>>,
    active: Cell<bool>,
}

impl<E> Default for EffectRoute<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E> EffectRoute<E> {
    pub const fn new() -> Self {
        Self {
            handler: RefCell::new(None),
            active: Cell::new(false),
        }
    }

    pub fn register(&self, handler: impl Fn(E) + 'static) -> Result<(), EffectRegistrationError> {
        if self.active.get() || self.handler.borrow().is_some() {
            return Err(EffectRegistrationError::AlreadyRegistered);
        }
        *self.handler.borrow_mut() = Some(Box::new(handler));
        Ok(())
    }

    pub fn deliver(&self, event: E) {
        assert!(
            !self.active.get(),
            "effect consumer cannot recursively deliver"
        );
        let Some(handler) = self.handler.borrow_mut().take() else {
            return;
        };
        self.active.set(true);
        struct Restore<'a, E> {
            route: &'a EffectRoute<E>,
            handler: Option<EffectHandler<E>>,
        }
        impl<E> Drop for Restore<'_, E> {
            fn drop(&mut self) {
                *self.route.handler.borrow_mut() = self.handler.take();
                self.route.active.set(false);
            }
        }
        let restore = Restore {
            route: self,
            handler: Some(handler),
        };
        let _read_only = crate::core::reactive::ModelReadOnlyGuard::enter();
        (restore.handler.as_ref().expect("effect handler"))(event);
    }
}

/// A model type that can create its generated instance handle.
pub trait Model: ModelMethods + Sized + 'static {
    type Handle: ModelHandle<Data = Self>;
    type Snapshot: Copy;
    type Sources: AsRef<[crate::core::reactive::ModelSource]>;
    type Change: Copy;
    type Watches: AsRef<[ModelWatch]>;

    #[doc(hidden)]
    fn handle(cell: Weak<ModelCell<Self>>) -> Self::Handle;

    #[doc(hidden)]
    fn snapshot(&self) -> Self::Snapshot;

    #[doc(hidden)]
    fn sources() -> Self::Sources;

    #[doc(hidden)]
    fn publish(sources: &Self::Sources, before: Self::Snapshot, after: Self::Snapshot);

    #[doc(hidden)]
    fn watches() -> Self::Watches;

    #[doc(hidden)]
    fn publish_change(watches: &Self::Watches, change: Self::Change);
}

/// Maps a declaration type to the value stored by a bound callback or component.
pub trait BindType {
    type Shared: SharedValue;
}

/// A cheap, instance-preserving value accepted by generated binding captures.
pub trait SharedValue: Clone {
    fn share(&self) -> Self {
        self.clone()
    }
}

impl<T: BindType> BindType for Option<T> {
    type Shared = Option<T::Shared>;
}

impl<T: SharedValue> SharedValue for Option<T> {}

/// A named revision source owned by one registered model instance.
#[doc(hidden)]
pub struct ModelWatch {
    revision: Cell<u64>,
    source: crate::core::reactive::ModelSource,
}

/// One explicit visual subscription to a registered model source.
#[doc(hidden)]
pub struct ModelSubscription {
    owner: Weak<dyn ModelSubscriptionOwner>,
    kind: SourceKind,
    index: usize,
    id: u64,
}

impl Drop for ModelSubscription {
    fn drop(&mut self) {
        if let Some(owner) = self.owner.upgrade() {
            owner.unsubscribe_source(self.kind, self.index, self.id);
        }
    }
}

trait ModelSubscriptionOwner {
    fn unsubscribe_source(&self, kind: SourceKind, index: usize, id: u64);
}

impl Default for ModelWatch {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelWatch {
    pub const fn new() -> Self {
        Self {
            revision: Cell::new(0),
            source: crate::core::reactive::ModelSource::new(),
        }
    }

    pub fn revision(&self) -> u64 {
        self.source.track();
        self.revision.get()
    }

    pub fn publish(&self) {
        self.revision.set(self.revision.get().wrapping_add(1));
        self.source.notify();
    }

    fn register_owner(&self, owner: WorldId) {
        self.source.register_owner(owner);
    }

    fn invalidate_computeds(&self) {
        self.source.invalidate_computeds();
    }

    fn subscribe_visual_widget(&self, owner: WorldId, entity: crate::ecs::Entity) -> u64 {
        self.source.subscribe_visual_widget(owner, entity)
    }

    fn unsubscribe(&self, id: u64) {
        self.source.unsubscribe(id);
    }
}

impl<T: 'static> BindType for crate::core::reactive::Signal<T> {
    type Shared = Self;
}

impl<T: 'static> SharedValue for crate::core::reactive::Signal<T> {}

impl<T: 'static> BindType for crate::core::reactive::Computed<T> {
    type Shared = Self;
}

impl<T: 'static> SharedValue for crate::core::reactive::Computed<T> {}

/// Storage owned by a model's registration entity.
#[doc(hidden)]
pub struct ModelCell<M: Model> {
    owner: WorldId,
    active: Cell<bool>,
    value: RefCell<M>,
    sources: M::Sources,
    derived_sources: M::DerivedSources,
    watches: M::Watches,
    routes: M::Routes,
    poisoned: Cell<bool>,
}

impl<M: Model> ModelCell<M> {
    pub(crate) fn new(owner: WorldId, value: M) -> Self {
        let sources = M::sources();
        let derived_sources = M::derived_sources();
        let watches = M::watches();
        for source in sources.as_ref().iter().chain(derived_sources.as_ref()) {
            source.register_owner(owner);
        }
        for watch in watches.as_ref() {
            watch.register_owner(owner);
        }
        Self {
            owner,
            active: Cell::new(true),
            value: RefCell::new(value),
            sources,
            derived_sources,
            watches,
            routes: M::routes(),
            poisoned: Cell::new(false),
        }
    }

    pub(crate) fn owner(&self) -> WorldId {
        self.owner
    }

    fn assert_active(&self) {
        assert!(self.active.get(), "model registration is no longer alive");
    }
}

impl<M: Model> ModelSubscriptionOwner for ModelCell<M> {
    fn unsubscribe_source(&self, kind: SourceKind, index: usize, id: u64) {
        match kind {
            SourceKind::Observed => self.sources.as_ref()[index].unsubscribe(id),
            SourceKind::Derived => self.derived_sources.as_ref()[index].unsubscribe(id),
            SourceKind::Watch => self.watches.as_ref()[index].unsubscribe(id),
        }
    }
}

pub(crate) struct RegisteredModel<M: Model> {
    cell: Rc<ModelCell<M>>,
}

impl<M: Model> Drop for RegisteredModel<M> {
    fn drop(&mut self) {
        self.cell.active.set(false);
        for source in self
            .cell
            .sources
            .as_ref()
            .iter()
            .chain(self.cell.derived_sources.as_ref())
        {
            source.invalidate_computeds();
        }
        for watch in self.cell.watches.as_ref() {
            watch.invalidate_computeds();
        }
    }
}

struct UpdateGuard<'a> {
    poisoned: &'a Cell<bool>,
    committed: bool,
}

impl Drop for UpdateGuard<'_> {
    fn drop(&mut self) {
        if !self.committed {
            self.poisoned.set(true);
        }
    }
}

/// A handle to one registered model instance.
pub trait ModelHandle: Clone {
    type Data: Model<Handle = Self>;

    #[doc(hidden)]
    fn cell(&self) -> &Weak<ModelCell<Self::Data>>;

    #[doc(hidden)]
    fn subscribe_observed(
        &self,
        index: usize,
        world: &crate::ecs::World,
        entity: crate::ecs::Entity,
    ) -> ModelSubscription {
        subscribe_visual(self, index, world, entity, SourceKind::Observed)
    }

    #[doc(hidden)]
    fn subscribe_derived(
        &self,
        index: usize,
        world: &crate::ecs::World,
        entity: crate::ecs::Entity,
    ) -> ModelSubscription {
        subscribe_visual(self, index, world, entity, SourceKind::Derived)
    }

    #[doc(hidden)]
    fn subscribe_watch(
        &self,
        index: usize,
        world: &crate::ecs::World,
        entity: crate::ecs::Entity,
    ) -> ModelSubscription {
        subscribe_visual(self, index, world, entity, SourceKind::Watch)
    }

    #[doc(hidden)]
    fn read<R>(&self, read: impl FnOnce(&Self::Data) -> R) -> R {
        let cell = self
            .cell()
            .upgrade()
            .expect("model registration is no longer alive");
        cell.assert_active();
        if let Some(active) = crate::core::reactive::current_model_read_owner() {
            assert_eq!(active, cell.owner(), "model belongs to a different App");
        }
        assert!(
            !cell.poisoned.get(),
            "model was poisoned by a failed update"
        );
        let value = cell
            .value
            .try_borrow()
            .expect("model is already mutably borrowed");
        read(&value)
    }

    #[doc(hidden)]
    fn read_observed<R>(&self, index: usize, read: impl FnOnce(&Self::Data) -> R) -> R {
        let cell = self
            .cell()
            .upgrade()
            .expect("model registration is no longer alive");
        self.read(|value| {
            cell.sources.as_ref()[index].track();
            read(value)
        })
    }

    #[doc(hidden)]
    fn read_derived<R>(&self, index: usize, read: impl FnOnce(&Self::Data) -> R) -> R {
        let cell = self
            .cell()
            .upgrade()
            .expect("model registration is no longer alive");
        self.read(|value| {
            cell.derived_sources.as_ref()[index].track();
            read(value)
        })
    }

    #[doc(hidden)]
    fn watch_revision(&self, index: usize) -> u64 {
        let cell = self
            .cell()
            .upgrade()
            .expect("model registration is no longer alive");
        self.read(|_| cell.watches.as_ref()[index].revision())
    }

    #[doc(hidden)]
    fn update(
        &self,
        update: impl FnOnce(&mut Self::Data) -> <Self::Data as Model>::Change,
    ) -> <Self::Data as Model>::Change {
        assert!(
            crate::core::reactive::model_writes_allowed(),
            "model updates are not allowed in a read-only callback"
        );
        let cell = self
            .cell()
            .upgrade()
            .expect("model registration is no longer alive");
        cell.assert_active();
        if let Some(active) = crate::core::reactive::current_world_id() {
            assert_eq!(active, cell.owner(), "model belongs to a different App");
        }
        assert!(
            !cell.poisoned.get(),
            "model was poisoned by a failed update"
        );
        let mut value = cell
            .value
            .try_borrow_mut()
            .expect("model is already borrowed");
        let mut guard = UpdateGuard {
            poisoned: &cell.poisoned,
            committed: false,
        };
        let before = value.snapshot();
        let derived_before = value.derived_snapshot();
        let result = update(&mut value);
        let events = value.take_events();
        let after = value.snapshot();
        let derived_after = value.derived_snapshot();
        drop(value);
        Self::Data::publish(&cell.sources, before, after);
        Self::Data::publish_derived(&cell.derived_sources, derived_before, derived_after);
        Self::Data::publish_change(&cell.watches, result);
        guard.committed = true;
        Self::Data::deliver_events(&cell.routes, events);
        result
    }
}

/// Run a generated View callback without permitting model writes.
#[doc(hidden)]
pub fn with_model_read_only<R>(run: impl FnOnce() -> R) -> R {
    let _guard = crate::core::reactive::ModelReadOnlyGuard::enter();
    run()
}

#[derive(Clone, Copy)]
enum SourceKind {
    Observed,
    Derived,
    Watch,
}

fn subscribe_visual<H: ModelHandle>(
    handle: &H,
    index: usize,
    world: &crate::ecs::World,
    entity: crate::ecs::Entity,
    kind: SourceKind,
) -> ModelSubscription {
    assert!(
        world.is_alive(entity),
        "visual subscription entity is not alive"
    );
    let cell = handle
        .cell()
        .upgrade()
        .expect("model registration is no longer alive");
    cell.assert_active();
    assert_eq!(cell.owner(), world.id(), "model belongs to a different App");
    let id = match kind {
        SourceKind::Observed => {
            cell.sources.as_ref()[index].subscribe_visual_widget(world.id(), entity)
        }
        SourceKind::Derived => {
            cell.derived_sources.as_ref()[index].subscribe_visual_widget(world.id(), entity)
        }
        SourceKind::Watch => {
            cell.watches.as_ref()[index].subscribe_visual_widget(world.id(), entity)
        }
    };
    let owner: Rc<dyn ModelSubscriptionOwner> = cell;
    ModelSubscription {
        owner: Rc::downgrade(&owner),
        kind,
        index,
        id,
    }
}

pub(crate) fn register<M: Model>(
    world: &mut crate::ecs::World,
    value: M,
) -> (RegisteredModel<M>, M::Handle) {
    let _owner = crate::core::reactive::OwnerGuard::enter(world);
    let cell = Rc::new(ModelCell::new(world.id(), value));
    let handle = M::handle(Rc::downgrade(&cell));
    (RegisteredModel { cell }, handle)
}

pub(crate) fn registered_owner<H: ModelHandle>(handle: &H) -> WorldId {
    let cell = handle
        .cell()
        .upgrade()
        .expect("model registration is no longer alive");
    cell.assert_active();
    cell.owner()
}

pub(crate) fn register_effect<H, E>(
    owner: WorldId,
    handle: &H,
    handler: impl Fn(E) + 'static,
) -> Result<(), EffectRegistrationError>
where
    H: ModelHandle,
    H::Data: Produces<E>,
{
    let cell = handle
        .cell()
        .upgrade()
        .expect("model registration is no longer alive");
    cell.assert_active();
    assert_eq!(cell.owner(), owner, "model belongs to a different App");
    <H::Data as Produces<E>>::route(&cell.routes).register(handler)
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::{Model, ModelCell, ModelHandle, ModelMethods};
    use crate::core::reactive::{Computed, Effect, flush_signal_dirty, with_world_scope};
    use crate::ui::dirty::VisualDirty;
    use alloc::rc::{Rc, Weak};
    use core::cell::Cell;

    struct Counter(u32);

    struct CounterHandle(Weak<ModelCell<Counter>>);

    impl ModelMethods for Counter {
        type DerivedSnapshot = ();
        type DerivedSources = [crate::core::reactive::ModelSource; 0];
        type Events = ();
        type Routes = ();

        fn derived_snapshot(&self) -> Self::DerivedSnapshot {}

        fn derived_sources() -> Self::DerivedSources {
            []
        }

        fn publish_derived(
            _: &Self::DerivedSources,
            _: Self::DerivedSnapshot,
            _: Self::DerivedSnapshot,
        ) {
        }

        fn take_events(&mut self) -> Self::Events {}

        fn routes() -> Self::Routes {}

        fn deliver_events(_: &Self::Routes, _: Self::Events) {}
    }

    impl Clone for CounterHandle {
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }

    impl Model for Counter {
        type Handle = CounterHandle;
        type Snapshot = u32;
        type Sources = [crate::core::reactive::ModelSource; 1];
        type Change = ();
        type Watches = [super::ModelWatch; 0];

        fn handle(cell: Weak<ModelCell<Self>>) -> Self::Handle {
            CounterHandle(cell)
        }

        fn snapshot(&self) -> Self::Snapshot {
            self.0
        }

        fn sources() -> Self::Sources {
            [crate::core::reactive::ModelSource::new()]
        }

        fn publish(sources: &Self::Sources, before: Self::Snapshot, after: Self::Snapshot) {
            if before != after {
                sources[0].notify();
            }
        }

        fn watches() -> Self::Watches {
            []
        }

        fn publish_change(_: &Self::Watches, _: Self::Change) {}
    }

    impl ModelHandle for CounterHandle {
        type Data = Counter;

        fn cell(&self) -> &Weak<ModelCell<Counter>> {
            &self.0
        }
    }

    impl CounterHandle {
        fn increment(&self) {
            self.update(|counter| counter.0 += 1);
        }

        fn value(&self) -> u32 {
            self.read_observed(0, |counter| counter.0)
        }
    }

    #[test]
    fn registered_instances_are_independent_and_clones_share_one_instance() {
        let mut app = crate::app::App::headless(32, 32);
        let first = app.add_model(Counter(1));
        let second = app.add_model(Counter(8));
        let first_copy = first.clone();

        first.increment();
        first_copy.increment();
        second.increment();

        assert_eq!(first.value(), 3);
        assert_eq!(first_copy.value(), 3);
        assert_eq!(second.value(), 9);
    }

    #[test]
    fn handle_rejects_access_after_app_drops() {
        let handle = {
            let mut app = crate::app::App::headless(32, 32);
            app.add_model(Counter(4))
        };
        let error = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handle.value()));
        assert!(error.is_err());
    }

    #[test]
    fn direct_model_registration_installs_world_owner_cleanup() {
        let mut world = crate::ecs::World::new();
        let owner = world.id();
        let (registration, _handle) = super::register(&mut world, Counter(4));
        assert!(crate::core::reactive::has_graph_owner(owner));

        drop(world);
        assert!(!crate::core::reactive::has_graph_owner(owner));
        drop(registration);
    }

    #[test]
    fn retained_cell_does_not_keep_a_registration_alive() {
        let (handle, retained_cell) = {
            let mut app = crate::app::App::headless(32, 32);
            let handle = app.add_model(Counter(4));
            let retained_cell = handle.cell().upgrade().unwrap();
            (handle, retained_cell)
        };
        let error = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handle.value()));
        assert!(error.is_err());
        drop(retained_cell);
    }

    #[test]
    fn despawning_registration_invalidates_retained_handles() {
        let mut app = crate::app::App::headless(32, 32);
        let handle = app.add_model(Counter(4));
        let retained_cell = handle.cell().upgrade().unwrap();
        let registration = app
            .world
            .query::<super::RegisteredModel<Counter>>()
            .iter()
            .next()
            .unwrap()
            .0;

        assert!(app.world.despawn(registration));
        let read = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handle.value()));
        let write = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handle.increment()));
        assert!(read.is_err());
        assert!(write.is_err());
        drop(retained_cell);
    }

    #[test]
    fn despawned_model_invalidates_cached_computeds_without_rerunning_effects() {
        let mut app = crate::app::App::headless(32, 32);
        let handle = app.add_model(Counter(4));
        let retained_cell = handle.cell().upgrade().unwrap();
        let source = handle.clone();
        let computed = with_world_scope(&mut app.world, || Computed::new(move || source.value()));
        let upstream = computed.clone();
        let downstream =
            with_world_scope(&mut app.world, || Computed::new(move || upstream.get() * 2));
        let runs = Rc::new(Cell::new(0u32));
        let read = downstream.clone();
        let count = Rc::clone(&runs);
        let _effect = with_world_scope(&mut app.world, || {
            Effect::new(move || {
                let _ = read.get();
                count.set(count.get() + 1);
            })
        });
        assert_eq!((computed.get(), downstream.get(), runs.get()), (4, 8, 1));
        let registration = app
            .world
            .query::<super::RegisteredModel<Counter>>()
            .iter()
            .next()
            .unwrap()
            .0;

        assert!(app.world.despawn(registration));
        flush_signal_dirty(&mut app.world);
        assert_eq!(runs.get(), 1, "cleanup must not schedule the stale Effect");
        let read = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| computed.get()));
        let downstream_read =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| downstream.get()));
        assert!(
            read.is_err(),
            "Computed must not return a stale cached value"
        );
        assert!(
            downstream_read.is_err(),
            "downstream Computed must also be dirty"
        );
        drop(retained_cell);
    }

    #[test]
    fn explicit_visual_subscriptions_release_one_registration_at_a_time() {
        let mut app = crate::app::App::headless(32, 32);
        let handle = app.add_model(Counter(0));
        let entity = app.world.spawn_empty();
        let first = handle.subscribe_observed(0, &app.world, entity);
        let second = handle.subscribe_observed(0, &app.world, entity);

        handle.increment();
        flush_signal_dirty(&mut app.world);
        assert!(app.world.remove::<VisualDirty>(entity).is_some());

        drop(first);
        handle.increment();
        flush_signal_dirty(&mut app.world);
        assert!(app.world.remove::<VisualDirty>(entity).is_some());

        drop(second);
        handle.increment();
        flush_signal_dirty(&mut app.world);
        assert!(!app.world.has::<VisualDirty>(entity));
    }

    #[test]
    fn nested_write_is_rejected_and_failed_update_poisoned() {
        let mut app = crate::app::App::headless(32, 32);
        let handle = app.add_model(Counter(1));
        let nested = handle.clone();
        let error = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            handle.update(|counter| {
                counter.0 = 2;
                nested.increment();
            });
        }));
        assert!(error.is_err());
        let read_after_failure =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handle.value()));
        assert!(read_after_failure.is_err());
    }

    #[test]
    fn active_other_app_cannot_access_a_model() {
        let mut first_app = crate::app::App::headless(32, 32);
        let first = first_app.add_model(Counter(1));
        let mut second_app = crate::app::App::headless(32, 32);
        let error = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = crate::core::reactive::OwnerGuard::enter(&mut second_app.world);
            first.increment();
        }));
        assert!(error.is_err());
        assert_eq!(first.value(), 1);
    }

    #[test]
    fn named_revision_wraps_without_stalling_notifications() {
        let watch = super::ModelWatch::new();
        watch.revision.set(u64::MAX);
        watch.publish();
        assert_eq!(watch.revision(), 0);
        watch.publish();
        assert_eq!(watch.revision(), 1);
    }
}
