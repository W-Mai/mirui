use super::{
    BindingOwner, Model, ModelAccess, ModelMetadata, ModelPath, ModelRoot, OwnedStorage, OwnerState,
};
use alloc::rc::{Rc, Weak};

/// Statically declared child identities and their transaction state.
#[doc(hidden)]
pub trait ModelChildren<M: Model>: Sized {
    type Snapshot: Copy;
    type Events;
    type Pending;

    fn register<R: Model, P: ModelPath<R, Target = M>>(
        root: &Weak<ModelRoot<R>>,
        owner: BindingOwner,
        path: P,
    ) -> Self;
    fn snapshot(value: &M) -> Self::Snapshot;
    fn take_events(value: &mut M) -> Self::Events;
    fn finish(&self, value: &mut M, reset: bool) -> Self::Pending;
    fn publish(&self, before: Self::Snapshot, after: Self::Snapshot, pending: Self::Pending);
    fn deliver(&self, events: Self::Events);
    fn invalidate(&self);
}

impl<M: Model> ModelChildren<M> for () {
    type Snapshot = ();
    type Events = ();
    type Pending = ();

    fn register<R: Model, P: ModelPath<R, Target = M>>(
        _: &Weak<ModelRoot<R>>,
        _: BindingOwner,
        _: P,
    ) {
    }
    fn snapshot(_: &M) {}
    fn take_events(_: &mut M) {}
    fn finish(&self, _: &mut M, _: bool) {}
    fn publish(&self, _: (), _: (), _: ()) {}
    fn deliver(&self, _: ()) {}
    fn invalidate(&self) {}
}

/// The observed values of a statically declared model subtree.
#[doc(hidden)]
pub struct ModelSnapshot<M: Model> {
    observed: M::Snapshot,
    derived: M::DerivedSnapshot,
    children: <M::Children as ModelChildren<M>>::Snapshot,
}

impl<M: Model> Copy for ModelSnapshot<M> {}

impl<M: Model> Clone for ModelSnapshot<M> {
    fn clone(&self) -> Self {
        *self
    }
}

/// Extracted fixed-capacity effects from one model subtree.
#[doc(hidden)]
pub struct ModelEvents<M: Model> {
    own: M::Events,
    children: <M::Children as ModelChildren<M>>::Events,
}

/// Pending watch notifications from one model subtree.
#[doc(hidden)]
pub struct ModelPending<M: Model> {
    own: M::PendingWatches,
    children: <M::Children as ModelChildren<M>>::Pending,
}

/// Registered identity and observations for one owned model field.
#[doc(hidden)]
pub struct ChildNode<M: Model> {
    metadata: ModelMetadata<M>,
    access: Rc<dyn ModelAccess<M>>,
}

impl<M: Model> ChildNode<M> {
    pub fn new<R: Model, P: ModelPath<R, Target = M>>(
        root: &Weak<ModelRoot<R>>,
        owner: BindingOwner,
        path: P,
    ) -> Self {
        Self {
            metadata: ModelMetadata::register::<R, P>(root, owner, path),
            access: Rc::new(super::projection::Projection::<R, P> {
                root: root.clone(),
                path,
            }),
        }
    }

    pub fn metadata(&self) -> &ModelMetadata<M> {
        &self.metadata
    }

    pub fn handle(&self) -> M::Handle {
        M::handle(Rc::downgrade(&self.access))
    }

    pub fn finish(&self, storage: &mut OwnedStorage<M>, reset: bool) -> ModelPending<M> {
        let identity = Rc::downgrade(&self.access);
        let moved = storage
            .identity
            .as_ref()
            .is_none_or(|old| !Weak::ptr_eq(old, &identity));
        if moved {
            storage.identity = Some(identity);
        }
        let pending = core::mem::replace(&mut storage.pending, M::pending_watches());
        self.metadata
            .finish(&mut storage.value, pending, reset || moved)
    }

    pub fn publish(
        &self,
        before: ModelSnapshot<M>,
        after: ModelSnapshot<M>,
        pending: ModelPending<M>,
    ) {
        self.metadata.publish(before, after, pending);
    }

    pub fn deliver(&self, events: ModelEvents<M>) {
        self.metadata.deliver(events);
    }

    pub fn invalidate(&self) {
        self.metadata.invalidate_computeds();
    }
}

impl<M: Model> ModelMetadata<M> {
    pub(super) fn register<R: Model, P: ModelPath<R, Target = M>>(
        root: &Weak<ModelRoot<R>>,
        owner: BindingOwner,
        path: P,
    ) -> Self {
        let OwnerState::Bound(world) = owner.0 else {
            panic!("model metadata requires a registered owner");
        };
        let sources = M::sources();
        let derived_sources = M::derived_sources();
        let watches = M::watches();
        for source in sources.as_ref().iter().chain(derived_sources.as_ref()) {
            source.register_owner(world);
        }
        for watch in watches.as_ref() {
            watch.register_owner(world);
        }
        Self {
            sources,
            derived_sources,
            watches,
            routes: M::routes(),
            children: M::Children::register::<R, P>(root, owner, path),
        }
    }

    #[doc(hidden)]
    pub fn children(&self) -> &M::Children {
        &self.children
    }

    #[doc(hidden)]
    pub fn snapshot(value: &M) -> ModelSnapshot<M> {
        ModelSnapshot {
            observed: value.snapshot(),
            derived: value.derived_snapshot(),
            children: M::Children::snapshot(value),
        }
    }

    pub(super) fn take_events(value: &mut M) -> ModelEvents<M> {
        ModelEvents {
            own: value.take_events(),
            children: M::Children::take_events(value),
        }
    }

    pub(super) fn finish(
        &self,
        value: &mut M,
        mut pending: M::PendingWatches,
        reset: bool,
    ) -> ModelPending<M> {
        if reset {
            pending.as_mut().fill(true);
        }
        ModelPending {
            own: pending,
            children: self.children.finish(value, reset),
        }
    }

    pub(super) fn publish(
        &self,
        before: ModelSnapshot<M>,
        after: ModelSnapshot<M>,
        pending: ModelPending<M>,
    ) {
        M::publish(&self.sources, before.observed, after.observed);
        M::publish_derived(&self.derived_sources, before.derived, after.derived);
        for (watch, changed) in self.watches.as_ref().iter().zip(pending.own.as_ref()) {
            if *changed {
                watch.publish();
            }
        }
        self.children
            .publish(before.children, after.children, pending.children);
    }

    pub(super) fn deliver(&self, events: ModelEvents<M>) {
        M::deliver_events(&self.routes, events.own);
        self.children.deliver(events.children);
    }

    pub(super) fn invalidate_computeds(&self) {
        for source in self
            .sources
            .as_ref()
            .iter()
            .chain(self.derived_sources.as_ref())
        {
            source.invalidate_computeds();
        }
        for watch in self.watches.as_ref() {
            watch.invalidate_computeds();
        }
        self.children.invalidate();
    }
}

#[cfg(all(test, feature = "std"))]
mod tests;
