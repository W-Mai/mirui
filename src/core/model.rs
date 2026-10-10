//! Registered model instances, typed projections, and retained transaction storage.

use alloc::boxed::Box;
use alloc::rc::Rc;
use core::cell::{Cell, RefCell};

use crate::ecs::world::WorldId;

mod collection;
mod identity;
mod list;
mod map;
mod owned;
mod projection;
mod set;
mod tree;

pub use collection::{CollectionError, CollectionLoadError, ModelItemEffect, ModelItemId};
pub use list::ModelList;
#[doc(hidden)]
pub use list::{ModelListEntries, ModelListHandle, ModelListOwned};
pub use map::ModelMap;
#[doc(hidden)]
pub use map::{ModelMapHandle, ModelMapOwned};
pub use set::ModelSet;
#[doc(hidden)]
pub use set::{ModelSetHandle, ModelSetOwned};

#[doc(hidden)]
pub use identity::ModelRef;
#[doc(hidden)]
pub use owned::{OwnedModel, OwnedStorage};
#[doc(hidden)]
pub use projection::{ModelChildField, ModelPath, ModelPathThen};
#[doc(hidden)]
pub use tree::{ChildNode, ModelChildren};

/// Derived observations declared by a model's inherent implementation.
#[doc(hidden)]
pub trait ModelMethods: Sized {
    type DerivedSnapshot: Copy;
    type DerivedSources: AsRef<[crate::core::reactive::ModelSource]>;
    type Events;
    type Routes;
    type ItemRoutes;

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
    fn retire_routes(routes: &Self::Routes);

    #[doc(hidden)]
    fn item_routes() -> Self::ItemRoutes;

    #[doc(hidden)]
    fn retire_item_routes(routes: &Self::ItemRoutes);

    #[doc(hidden)]
    fn deliver_events(
        routes: &Self::Routes,
        events: Self::Events,
        item: Option<(ModelItemId, &Self::ItemRoutes)>,
    );
}

/// A model declares that it produces one effect type.
pub trait Produces<E>: Model {
    #[doc(hidden)]
    fn route(metadata: &ModelMetadata<Self>) -> &EffectRoute<E>;
}

/// A declared local effect that can be aggregated by a containing collection.
#[doc(hidden)]
pub trait ItemEffectSource<E>: Produces<E> {
    fn item_route(routes: &Self::ItemRoutes) -> &EffectRoute<ModelItemEffect<E>>;
}

/// Registration failure for an effect consumer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectRegistrationError {
    AlreadyRegistered,
}

type EffectHandler<E> = Box<dyn Fn(&E)>;

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

    pub fn register(&self, handler: impl Fn(&E) + 'static) -> Result<(), EffectRegistrationError> {
        if self.active.get() || self.handler.borrow().is_some() {
            return Err(EffectRegistrationError::AlreadyRegistered);
        }
        *self.handler.borrow_mut() = Some(Box::new(handler));
        Ok(())
    }

    pub fn retire(&self) {
        self.active.set(false);
        let handler = self.handler.borrow_mut().take();
        drop(handler);
    }

    pub fn deliver(&self, event: &E) {
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
                if self.route.active.replace(false) {
                    *self.route.handler.borrow_mut() = self.handler.take();
                }
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
    type Owned: OwnedModel<Data = Self> + From<Self>;
    type Snapshot: Copy;
    type Sources: AsRef<[crate::core::reactive::ModelSource]>;
    type Change: Copy;
    type Watches: AsRef<[ModelWatch]>;
    type PendingWatches: Copy + AsRef<[bool]> + AsMut<[bool]>;
    type Children: ModelChildren<Self>;

    #[doc(hidden)]
    fn handle(access: ModelRef<Self>) -> Self::Handle;

    #[doc(hidden)]
    fn snapshot(&self) -> Self::Snapshot;

    #[doc(hidden)]
    fn sources() -> Self::Sources;

    #[doc(hidden)]
    fn publish(sources: &Self::Sources, before: Self::Snapshot, after: Self::Snapshot);

    #[doc(hidden)]
    fn watches() -> Self::Watches;

    #[doc(hidden)]
    fn pending_watches() -> Self::PendingWatches;

    #[doc(hidden)]
    fn record_change(pending: &mut Self::PendingWatches, change: Self::Change);
}

/// Maps a declaration type to the value stored by a bound callback or component.
pub trait BindType {
    type Shared: SharedValue;
}

/// An invalid registered model identity captured by a binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum ModelOwnerError {
    RegistrationEnded,
    MixedOwners,
    ForeignOwner,
}

#[derive(Clone, Copy)]
enum OwnerState {
    Unbound,
    Bound(WorldId),
    Invalid(ModelOwnerError),
}

/// Opaque ownership requirements contributed by a captured shared value.
#[derive(Clone, Copy)]
#[doc(hidden)]
pub struct BindingOwner(OwnerState);

impl BindingOwner {
    /// A shared value with no registered model ownership requirement.
    #[doc(hidden)]
    pub const fn unbound() -> Self {
        Self(OwnerState::Unbound)
    }

    /// Capture a model's current registration identity without retaining its cell.
    #[doc(hidden)]
    pub fn registered<H: ModelHandle>(handle: &H) -> Self {
        match try_registered_owner(handle) {
            Some(owner) => Self(OwnerState::Bound(owner)),
            None => Self(OwnerState::Invalid(ModelOwnerError::RegistrationEnded)),
        }
    }

    pub(crate) fn merge(self, other: Self) -> Self {
        match (self.0, other.0) {
            (OwnerState::Invalid(_), _) => self,
            (_, OwnerState::Invalid(_)) | (OwnerState::Unbound, _) => other,
            (OwnerState::Bound(left), OwnerState::Bound(right)) if left != right => {
                Self(OwnerState::Invalid(ModelOwnerError::MixedOwners))
            }
            _ => self,
        }
    }

    pub(crate) fn check(self, owner: WorldId) -> Result<(), ModelOwnerError> {
        match self.0 {
            OwnerState::Invalid(error) => Err(error),
            OwnerState::Bound(required) if required != owner => Err(ModelOwnerError::ForeignOwner),
            OwnerState::Unbound | OwnerState::Bound(_) => Ok(()),
        }
    }

    pub(crate) fn is_invalid(self) -> bool {
        matches!(self.0, OwnerState::Invalid(_))
    }
}

/// A cheap, instance-preserving value accepted by generated binding captures.
pub trait SharedValue: Clone {
    #[doc(hidden)]
    fn binding_owner(&self) -> BindingOwner;

    #[doc(hidden)]
    fn callback_ready(&self) -> bool {
        true
    }

    #[doc(hidden)]
    fn assert_registered_owner(&self) {
        match self.binding_owner().0 {
            OwnerState::Unbound => {}
            OwnerState::Bound(owner) => assert_eq!(
                Some(owner),
                crate::core::reactive::current_world_id(),
                "model belongs to a different App",
            ),
            OwnerState::Invalid(ModelOwnerError::RegistrationEnded) => {
                panic!("model registration is no longer alive")
            }
            OwnerState::Invalid(ModelOwnerError::MixedOwners) => {
                panic!("model bindings belong to different Apps")
            }
            OwnerState::Invalid(ModelOwnerError::ForeignOwner) => {
                panic!("model belongs to a different App")
            }
        }
    }

    fn share(&self) -> Self {
        self.clone()
    }
}

impl<T: BindType> BindType for Option<T> {
    type Shared = Option<T::Shared>;
}

impl<T: SharedValue> SharedValue for Option<T> {
    fn callback_ready(&self) -> bool {
        self.as_ref().is_none_or(SharedValue::callback_ready)
    }
    fn binding_owner(&self) -> BindingOwner {
        self.as_ref()
            .map_or(BindingOwner::unbound(), SharedValue::binding_owner)
    }
}

/// A named revision source owned by one registered model instance.
#[doc(hidden)]
pub struct ModelWatch {
    revision: Cell<u64>,
    source: crate::core::reactive::ModelSource,
}

/// One explicit View subscription to a registered model source.
#[doc(hidden)]
pub struct ModelSubscription {
    _consumer: Option<crate::core::reactive::ModelViewSubscription>,
}

impl ModelSubscription {
    /// Fill one statically declared View observation slot without subscribing.
    #[doc(hidden)]
    pub const fn inert() -> Self {
        Self { _consumer: None }
    }
}

/// Dirty scope selected by the View that owns a model subscription.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelInvalidation {
    Visual,
    Layout,
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

    fn retire(&self) {
        self.source.retire();
    }

    fn can_reactivate(&self, owner: WorldId) -> bool {
        self.source.can_reactivate(owner)
    }

    fn reactivate(&self, owner: WorldId) {
        self.revision.set(0);
        self.source.register_owner(owner);
    }

    fn subscribe_view(
        &self,
        owner: WorldId,
        entity: crate::ecs::Entity,
        invalidation: ModelInvalidation,
    ) -> crate::core::reactive::ModelViewSubscription {
        match invalidation {
            ModelInvalidation::Visual => self.source.subscribe_visual_widget(owner, entity),
            ModelInvalidation::Layout => self.source.subscribe_widget(owner, entity),
        }
    }
}

impl<T: 'static> BindType for crate::core::reactive::Signal<T> {
    type Shared = Self;
}

impl<T: 'static> SharedValue for crate::core::reactive::Signal<T> {
    fn binding_owner(&self) -> BindingOwner {
        BindingOwner::unbound()
    }
}

impl<T: 'static> BindType for crate::core::reactive::Computed<T> {
    type Shared = Self;
}

impl<T: 'static> SharedValue for crate::core::reactive::Computed<T> {
    fn binding_owner(&self) -> BindingOwner {
        BindingOwner::unbound()
    }
}

/// Observation and effect state belonging to one registered model identity.
#[doc(hidden)]
pub struct ModelMetadata<M: Model> {
    owner: WorldId,
    lifetime: identity::ModelLifetime,
    sources: M::Sources,
    derived_sources: M::DerivedSources,
    watches: M::Watches,
    routes: M::Routes,
    children: M::Children,
    transaction: RefCell<tree::ModelTransaction<M>>,
}

type ModelUpdate<'a, M> = dyn FnMut(&mut M) -> Option<<M as Model>::Change> + 'a;

/// Borrowed access to a registered model's data and identity.
/// Each callback runs synchronously once while its references remain borrowed.
#[doc(hidden)]
pub trait ModelAccess<M: Model> {
    fn generation(&self) -> Option<u64>;

    fn binding_owner(&self) -> BindingOwner;

    fn read(&self, read: &mut dyn FnMut(&M, &ModelMetadata<M>));

    fn update(&self, update: &mut ModelUpdate<'_, M>);

    fn metadata(&self, read: &mut dyn FnMut(&ModelMetadata<M>));
}

/// The registered owner of one statically declared model tree.
#[doc(hidden)]
pub struct ModelRoot<M: Model> {
    value: RefCell<M>,
    metadata: ModelMetadata<M>,
    poisoned: Cell<bool>,
    updating: Cell<bool>,
}

impl<M: Model> ModelRoot<M> {
    fn new(owner: WorldId, mut value: M) -> Rc<Self> {
        Rc::new_cyclic(|root| {
            let metadata = ModelMetadata::register::<M, projection::RootPath>(
                root,
                BindingOwner(OwnerState::Bound(owner)),
                projection::RootPath,
                Some(&mut value),
            );
            Self {
                value: RefCell::new(value),
                metadata,
                poisoned: Cell::new(false),
                updating: Cell::new(false),
            }
        })
    }

    fn assert_active(&self) {
        assert!(
            self.generation().is_some(),
            "model registration is no longer alive"
        );
    }
}

pub(crate) struct RegisteredModel<M: Model> {
    root: Rc<ModelRoot<M>>,
}

impl<M: Model> Drop for RegisteredModel<M> {
    fn drop(&mut self) {
        self.root.metadata.retire();
    }
}

struct UpdateGuard<'a, M: Model> {
    poisoned: &'a Cell<bool>,
    updating: &'a Cell<bool>,
    metadata: &'a ModelMetadata<M>,
    committed: bool,
}

impl<M: Model> Drop for UpdateGuard<'_, M> {
    fn drop(&mut self) {
        if !self.committed {
            self.poisoned.set(true);
        }
        self.metadata.discard();
        self.updating.set(false);
    }
}

impl<M: Model> ModelAccess<M> for ModelRoot<M> {
    fn generation(&self) -> Option<u64> {
        self.metadata.lifetime.current()
    }

    fn binding_owner(&self) -> BindingOwner {
        if self.generation().is_some() {
            BindingOwner(OwnerState::Bound(self.metadata.owner))
        } else {
            BindingOwner(OwnerState::Invalid(ModelOwnerError::RegistrationEnded))
        }
    }

    fn read(&self, read: &mut dyn FnMut(&M, &ModelMetadata<M>)) {
        self.assert_active();
        if let Some(active) = crate::core::reactive::current_model_read_owner() {
            assert_eq!(
                active, self.metadata.owner,
                "model belongs to a different App"
            );
        }
        assert!(
            !self.poisoned.get(),
            "model was poisoned by a failed update"
        );
        let value = self
            .value
            .try_borrow()
            .expect("model is already mutably borrowed");
        read(&value, &self.metadata);
    }

    fn update(&self, update: &mut ModelUpdate<'_, M>) {
        self.update_path(&projection::RootPath, update);
    }

    fn metadata(&self, read: &mut dyn FnMut(&ModelMetadata<M>)) {
        self.assert_active();
        read(&self.metadata);
    }
}

impl<M: Model> ModelRoot<M> {
    fn update_path<P: ModelPath<M>>(&self, path: &P, update: &mut ModelUpdate<'_, P::Target>) {
        assert!(
            crate::core::reactive::model_writes_allowed(),
            "model updates are not allowed in a read-only callback"
        );
        self.assert_active();
        if let Some(active) = crate::core::reactive::current_world_id() {
            assert_eq!(
                active, self.metadata.owner,
                "model belongs to a different App"
            );
        }
        assert!(
            !self.poisoned.get(),
            "model was poisoned by a failed update"
        );
        assert!(
            path.metadata(&self.metadata, |metadata| metadata.children.is_writable()),
            "model list is being iterated"
        );
        let mut value = self
            .value
            .try_borrow_mut()
            .expect("model is already borrowed");
        assert!(!self.updating.replace(true), "model is already updating");
        let mut guard = UpdateGuard {
            poisoned: &self.poisoned,
            updating: &self.updating,
            metadata: &self.metadata,
            committed: false,
        };
        self.metadata.begin(&value);
        let mut watches = M::pending_watches();
        let accepted = path.update(&mut value, &mut watches, update);
        self.metadata.extract(&mut value);
        self.metadata.finish(&mut value, watches, false);
        if !accepted {
            drop(value);
            guard.committed = true;
            return;
        }
        self.metadata.capture_after(&value);
        drop(value);
        self.metadata.publish();
        guard.committed = true;
        self.metadata.deliver(None);
    }
}

/// A handle to one registered model instance.
pub trait ModelHandle: Clone {
    type Data: Model<Handle = Self>;

    /// Register one consumer for an effect type on this instance.
    ///
    /// Events are borrowed for the duration of the callback. Registration belongs to the Model, not to the composition that installs it. A second consumer for the same type is rejected.
    fn on_effect<E>(&self, handler: impl Fn(&E) + 'static) -> Result<(), EffectRegistrationError>
    where
        Self::Data: Produces<E>,
    {
        let owner = registered_owner(self);
        if let Some(active) = crate::core::reactive::current_world_id() {
            assert_eq!(active, owner, "model belongs to a different App");
        }
        read_metadata(self, owner, |metadata| {
            <Self::Data as Produces<E>>::route(metadata).register(handler)
        })
    }

    #[doc(hidden)]
    fn access(&self) -> &ModelRef<Self::Data>;

    #[doc(hidden)]
    fn callback_ready(&self) -> bool {
        self.access().callback_ready()
    }

    #[doc(hidden)]
    fn child<F: ModelChildField<Self::Data>>(&self) -> <F::Child as Model>::Handle {
        read_model(self, |_, metadata| F::node(metadata).handle())
    }

    #[doc(hidden)]
    fn subscribe_observed(
        &self,
        index: usize,
        world: &crate::ecs::World,
        entity: crate::ecs::Entity,
        invalidation: ModelInvalidation,
    ) -> ModelSubscription {
        subscribe_view(
            self,
            index,
            world,
            entity,
            SourceKind::Observed,
            invalidation,
        )
    }

    #[doc(hidden)]
    fn subscribe_derived(
        &self,
        index: usize,
        world: &crate::ecs::World,
        entity: crate::ecs::Entity,
        invalidation: ModelInvalidation,
    ) -> ModelSubscription {
        subscribe_view(
            self,
            index,
            world,
            entity,
            SourceKind::Derived,
            invalidation,
        )
    }

    #[doc(hidden)]
    fn subscribe_watch(
        &self,
        index: usize,
        world: &crate::ecs::World,
        entity: crate::ecs::Entity,
        invalidation: ModelInvalidation,
    ) -> ModelSubscription {
        subscribe_view(self, index, world, entity, SourceKind::Watch, invalidation)
    }

    #[doc(hidden)]
    fn read<R>(&self, read: impl FnOnce(&Self::Data) -> R) -> R {
        read_model(self, |value, _| read(value))
    }

    #[doc(hidden)]
    fn read_observed<R>(&self, index: usize, read: impl FnOnce(&Self::Data) -> R) -> R {
        read_model(self, |value, metadata| {
            metadata.sources.as_ref()[index].track();
            read(value)
        })
    }

    #[doc(hidden)]
    fn read_derived<R>(&self, index: usize, read: impl FnOnce(&Self::Data) -> R) -> R {
        read_model(self, |value, metadata| {
            metadata.derived_sources.as_ref()[index].track();
            read(value)
        })
    }

    #[doc(hidden)]
    fn watch_revision(&self, index: usize) -> u64 {
        read_model(self, |_, metadata| {
            metadata.watches.as_ref()[index].revision()
        })
    }

    #[doc(hidden)]
    fn update(
        &self,
        update: impl FnOnce(&mut Self::Data) -> <Self::Data as Model>::Change,
    ) -> <Self::Data as Model>::Change {
        update_model(self, update, |change| Some(*change))
    }

    #[doc(hidden)]
    fn try_update<E>(
        &self,
        update: impl FnOnce(&mut Self::Data) -> Result<<Self::Data as Model>::Change, E>,
    ) -> Result<<Self::Data as Model>::Change, E> {
        update_model(self, update, |result| result.as_ref().ok().copied())
    }
}

fn read_model<H: ModelHandle, R>(
    handle: &H,
    read: impl FnOnce(&H::Data, &ModelMetadata<H::Data>) -> R,
) -> R {
    let access = handle
        .access()
        .upgrade()
        .expect("model registration is no longer alive");
    let mut read = Some(read);
    let mut result = None;
    access.read(&mut |value, metadata| {
        result = Some(read.take().expect("model access called more than once")(
            value, metadata,
        ));
    });
    result.expect("model access did not call its reader")
}

fn update_model<H: ModelHandle, R>(
    handle: &H,
    update: impl FnOnce(&mut H::Data) -> R,
    accepted_change: impl FnOnce(&R) -> Option<<H::Data as Model>::Change>,
) -> R {
    let access = handle
        .access()
        .upgrade()
        .expect("model registration is no longer alive");
    let mut update = Some((update, accepted_change));
    let mut result = None;
    access.update(&mut |value| {
        let (update, accepted_change) = update.take().expect("model access called more than once");
        let updated = update(value);
        let change = accepted_change(&updated);
        result = Some(updated);
        change
    });
    result.expect("model access did not call its updater")
}

fn read_metadata<H: ModelHandle, R>(
    handle: &H,
    owner: WorldId,
    read: impl FnOnce(&ModelMetadata<H::Data>) -> R,
) -> R {
    let access = handle
        .access()
        .upgrade()
        .expect("model registration is no longer alive");
    let actual = match access.binding_owner().0 {
        OwnerState::Bound(owner) => owner,
        _ => panic!("model registration is no longer alive"),
    };
    assert_eq!(actual, owner, "model belongs to a different App");
    let mut read = Some(read);
    let mut result = None;
    access.metadata(&mut |metadata| {
        result = Some(read.take().expect("model access called more than once")(
            metadata,
        ));
    });
    result.expect("model access did not call its metadata reader")
}

/// Read a component field that may hold one registered model facade.
#[doc(hidden)]
pub trait OptionalModelHandle {
    type Data: Model;

    #[doc(hidden)]
    fn read_optional<R>(&self, read: impl FnOnce(Option<&Self::Data>) -> R) -> R;
}

impl<H: ModelHandle> OptionalModelHandle for Option<H> {
    type Data = H::Data;

    fn read_optional<R>(&self, read: impl FnOnce(Option<&Self::Data>) -> R) -> R {
        match self {
            Some(handle) => handle.read(|value| read(Some(value))),
            None => read(None),
        }
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

fn subscribe_source(
    source: &crate::core::reactive::ModelSource,
    owner: WorldId,
    entity: crate::ecs::Entity,
    invalidation: ModelInvalidation,
) -> crate::core::reactive::ModelViewSubscription {
    match invalidation {
        ModelInvalidation::Visual => source.subscribe_visual_widget(owner, entity),
        ModelInvalidation::Layout => source.subscribe_widget(owner, entity),
    }
}

fn subscribe_view<H: ModelHandle>(
    handle: &H,
    index: usize,
    world: &crate::ecs::World,
    entity: crate::ecs::Entity,
    kind: SourceKind,
    invalidation: ModelInvalidation,
) -> ModelSubscription {
    assert!(
        world.is_alive(entity),
        "visual subscription entity is not alive"
    );
    let consumer = read_metadata(handle, world.id(), |metadata| match kind {
        SourceKind::Observed => subscribe_source(
            &metadata.sources.as_ref()[index],
            world.id(),
            entity,
            invalidation,
        ),
        SourceKind::Derived => subscribe_source(
            &metadata.derived_sources.as_ref()[index],
            world.id(),
            entity,
            invalidation,
        ),
        SourceKind::Watch => {
            metadata.watches.as_ref()[index].subscribe_view(world.id(), entity, invalidation)
        }
    });
    ModelSubscription {
        _consumer: Some(consumer),
    }
}

pub(crate) fn register<M: Model>(
    world: &mut crate::ecs::World,
    value: M,
) -> (RegisteredModel<M>, M::Handle) {
    let _owner = crate::core::reactive::OwnerGuard::enter(world);
    let root = ModelRoot::new(world.id(), value);
    let access: Rc<dyn ModelAccess<M>> = root.clone();
    let handle = M::handle(ModelRef::new(
        &access,
        root.generation().expect("registered model"),
    ));
    (RegisteredModel { root }, handle)
}

pub(crate) fn registered_owner<H: ModelHandle>(handle: &H) -> WorldId {
    try_registered_owner(handle).expect("model registration is no longer alive")
}

pub(crate) fn try_registered_owner<H: ModelHandle>(handle: &H) -> Option<WorldId> {
    let access = handle.access().upgrade()?;
    match access.binding_owner().0 {
        OwnerState::Bound(owner) => Some(owner),
        _ => None,
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::{
        BindingOwner, Model, ModelHandle, ModelInvalidation, ModelMethods, ModelOwnerError,
        ModelRef, SharedValue,
    };
    use crate::core::reactive::{Computed, Effect, Signal, flush_signal_dirty, with_world_scope};
    use crate::test_support::tracked_allocations;
    use crate::ui::dirty::VisualDirty;
    use alloc::rc::Rc;
    use core::cell::Cell;

    struct Counter(u32);

    #[test]
    fn retiring_an_active_effect_route_cannot_restore_its_old_consumer() {
        for unwind in [false, true] {
            let route = Rc::new(super::EffectRoute::<()>::new());
            let weak = Rc::downgrade(&route);
            let deliveries = Rc::new(Cell::new(0));
            let counter = deliveries.clone();
            route
                .register(move |()| {
                    let route = weak.upgrade().unwrap();
                    route.retire();
                    let counter = counter.clone();
                    route
                        .register(move |()| counter.set(counter.get() + 1))
                        .unwrap();
                    assert!(!unwind, "effect unwind");
                })
                .unwrap();
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| route.deliver(&())));
            assert_eq!(result.is_err(), unwind);
            route.deliver(&());
            assert_eq!(deliveries.get(), 1);
            route.retire();
            route.deliver(&());
            assert_eq!(deliveries.get(), 1);
        }
    }

    struct CounterOwned(super::OwnedStorage<Counter>);

    impl From<Counter> for CounterOwned {
        fn from(value: Counter) -> Self {
            Self(super::OwnedStorage::new(value))
        }
    }

    impl super::OwnedModel for CounterOwned {
        type Data = Counter;

        fn storage(&self) -> &super::OwnedStorage<Counter> {
            &self.0
        }

        fn storage_mut(&mut self) -> &mut super::OwnedStorage<Counter> {
            &mut self.0
        }

        fn into_data(self) -> Counter {
            self.0.into_data()
        }
    }

    struct CounterHandle(ModelRef<Counter>);

    impl ModelMethods for Counter {
        type DerivedSnapshot = ();
        type DerivedSources = [crate::core::reactive::ModelSource; 0];
        type Events = ();
        type Routes = ();
        type ItemRoutes = ();

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

        fn retire_routes(_: &Self::Routes) {}

        fn item_routes() {}
        fn retire_item_routes(_: &Self::ItemRoutes) {}
        fn deliver_events(_: &Self::Routes, _: Self::Events, _: Option<(super::ModelItemId, &())>) {
        }
    }

    impl Clone for CounterHandle {
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }

    impl Model for Counter {
        type Handle = CounterHandle;
        type Owned = CounterOwned;
        type Snapshot = u32;
        type Sources = [crate::core::reactive::ModelSource; 1];
        type Change = ();
        type Watches = [super::ModelWatch; 0];
        type PendingWatches = [bool; 0];
        type Children = ();

        fn handle(access: ModelRef<Self>) -> Self::Handle {
            CounterHandle(access)
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

        fn pending_watches() -> Self::PendingWatches {
            []
        }

        fn record_change(_: &mut Self::PendingWatches, _: Self::Change) {}
    }

    impl ModelHandle for CounterHandle {
        type Data = Counter;

        fn access(&self) -> &ModelRef<Counter> {
            &self.0
        }
    }

    impl SharedValue for CounterHandle {
        fn binding_owner(&self) -> BindingOwner {
            BindingOwner::registered(self)
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
    fn access_callbacks_consume_stack_captures_once_without_allocating() {
        struct Payload<'a>(&'a Cell<u32>);

        impl Drop for Payload<'_> {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }

        let mut app = crate::app::App::headless(32, 32);
        let model = app.add_model(Counter(2));
        let reads = Cell::new(0);
        let updates = Cell::new(0);
        let read_payload = Payload(&reads);
        let update_payload = Payload(&updates);
        assert_eq!(
            tracked_allocations(|| {
                let returned = model.read(move |value| {
                    assert_eq!(value.0, 2);
                    read_payload
                });
                assert_eq!(reads.get(), 0);
                drop(returned);
                let result = model.try_update(move |value| {
                    value.0 += 1;
                    Err(update_payload)
                });
                assert_eq!(updates.get(), 0);
                drop(result.unwrap_err());
                assert_eq!(model.value(), 3);
                assert_eq!((reads.get(), updates.get()), (1, 1));
            }),
            0
        );
    }

    #[test]
    fn shared_binding_owners_merge_without_allocating_and_preserve_the_first_error() {
        const UNBOUND: BindingOwner = BindingOwner::unbound();
        let mut app = crate::app::App::headless(32, 32);
        let first = app.add_model(Counter(1));
        let second = app.add_model(Counter(2));
        let mut other = crate::app::App::headless(32, 32);
        let foreign = other.add_model(Counter(3));
        let ended = {
            let mut retired = crate::app::App::headless(32, 32);
            retired.add_model(Counter(4))
        };
        let absent: Option<Option<CounterHandle>> = None;
        let nested_absent = Some(None::<CounterHandle>);
        let present = Some(Some(first.clone()));
        let signal = Signal::new(5_u32);
        let computed = with_world_scope(&mut other.world, || Computed::new(|| 6_u32));
        let owner = app.world.id();

        assert_eq!(
            tracked_allocations(|| {
                for _ in 0..8 {
                    let bound = UNBOUND
                        .merge(absent.binding_owner())
                        .merge(present.binding_owner())
                        .merge(nested_absent.binding_owner())
                        .merge(second.binding_owner())
                        .merge(signal.binding_owner())
                        .merge(computed.binding_owner());
                    assert_eq!(bound.check(owner), Ok(()));
                    assert_eq!(
                        bound.check(other.world.id()),
                        Err(ModelOwnerError::ForeignOwner)
                    );
                    let mixed = bound.merge(foreign.binding_owner());
                    assert_eq!(mixed.check(owner), Err(ModelOwnerError::MixedOwners));
                    assert_eq!(
                        mixed.merge(ended.binding_owner()).check(owner),
                        Err(ModelOwnerError::MixedOwners)
                    );
                    assert_eq!(
                        ended.binding_owner().merge(mixed).check(owner),
                        Err(ModelOwnerError::RegistrationEnded)
                    );
                }
            }),
            0
        );
    }

    #[test]
    fn shared_binding_owner_assertion_uses_the_composition_scope_and_restores_the_outer_owner() {
        let mut app = crate::app::App::headless(32, 32);
        let root = app.spawn_root().id();
        let handle = app.add_model(Counter(1));
        let outside = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            handle.assert_registered_owner();
        }));
        assert!(outside.is_err());

        let mut other = crate::app::App::headless(32, 32);
        let foreign_handle = other.add_model(Counter(2));
        let other_owner = other.world.id();
        let _outer = crate::core::reactive::OwnerGuard::enter(&mut other.world);
        {
            let mut scope = crate::ui::UiScope::new(&mut app.world, root);
            scope.assert_binding(&Some(Some(handle.clone())));
        }
        assert_eq!(crate::core::reactive::current_world_id(), Some(other_owner));
        let foreign = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut scope = crate::ui::UiScope::new(&mut app.world, root);
            scope.assert_binding(&Some(Some(foreign_handle.clone())));
        }));
        assert!(foreign.is_err());
        assert_eq!(crate::core::reactive::current_world_id(), Some(other_owner));
    }

    #[test]
    fn handle_rejects_access_after_app_drops() {
        let (handle, owner, captured) = {
            let mut app = crate::app::App::headless(32, 32);
            let handle = app.add_model(Counter(4));
            let captured = handle.binding_owner();
            (handle, app.world.id(), captured)
        };
        assert_eq!(captured.check(owner), Ok(()));
        assert_eq!(
            handle.binding_owner().check(owner),
            Err(ModelOwnerError::RegistrationEnded)
        );
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
        let (handle, retained_cell, owner) = {
            let mut app = crate::app::App::headless(32, 32);
            let handle = app.add_model(Counter(4));
            let retained_cell = handle.access().upgrade().unwrap();
            (handle, retained_cell, app.world.id())
        };
        assert_eq!(
            handle.binding_owner().check(owner),
            Err(ModelOwnerError::RegistrationEnded)
        );
        let error = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handle.value()));
        assert!(error.is_err());
        drop(retained_cell);
    }

    #[test]
    fn despawning_registration_invalidates_retained_handles() {
        let mut app = crate::app::App::headless(32, 32);
        let handle = app.add_model(Counter(4));
        let retained_cell = handle.access().upgrade().unwrap();
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
        let retained_cell = handle.access().upgrade().unwrap();
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
        let first = handle.subscribe_observed(0, &app.world, entity, ModelInvalidation::Visual);
        let second = handle.subscribe_observed(0, &app.world, entity, ModelInvalidation::Visual);

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
    fn automatic_visual_scope_does_not_reuse_an_explicit_view_binding() {
        let mut app = crate::app::App::headless(32, 32);
        let handle = app.add_model(Counter(0));
        let entity = app.world.spawn_empty();
        let explicit = handle.subscribe_observed(0, &app.world, entity, ModelInvalidation::Visual);

        crate::core::reactive::with_world_scope(&mut app.world, || {
            crate::core::reactive::with_scope(
                crate::core::reactive::Subscriber::VisualWidget(entity),
                || {
                    assert_eq!(handle.value(), 0);
                },
            );
        });
        drop(explicit);

        handle.increment();
        flush_signal_dirty(&mut app.world);
        assert!(app.world.has::<VisualDirty>(entity));
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
        assert_eq!(handle.binding_owner().check(app.world.id()), Ok(()));
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
