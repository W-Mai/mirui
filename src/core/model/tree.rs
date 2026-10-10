use super::{BindingOwner, Model, ModelMetadata, OwnerState};

/// Statically declared child identities and their transaction state.
#[doc(hidden)]
pub trait ModelChildren<M: Model>: Sized {
    type Snapshot: Copy;
    type Events;
    type Pending;

    fn new(owner: BindingOwner) -> Self;
    fn snapshot(value: &M) -> Self::Snapshot;
    fn take_events(value: &mut M) -> Self::Events;
    fn finish(&self, value: &mut M) -> Self::Pending;
    fn publish(&self, before: Self::Snapshot, after: Self::Snapshot, pending: Self::Pending);
    fn deliver(&self, events: Self::Events);
    fn invalidate(&self);
}

impl<M: Model> ModelChildren<M> for () {
    type Snapshot = ();
    type Events = ();
    type Pending = ();

    fn new(_: BindingOwner) {}
    fn snapshot(_: &M) {}
    fn take_events(_: &mut M) {}
    fn finish(&self, _: &mut M) {}
    fn publish(&self, _: (), _: (), _: ()) {}
    fn deliver(&self, _: ()) {}
    fn invalidate(&self) {}
}

pub(super) struct ModelSnapshot<M: Model> {
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

pub(super) struct ModelEvents<M: Model> {
    own: M::Events,
    children: <M::Children as ModelChildren<M>>::Events,
}

impl<M: Model> ModelMetadata<M> {
    #[doc(hidden)]
    pub fn new(owner: BindingOwner) -> Self {
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
            children: M::Children::new(owner),
        }
    }

    pub(super) fn snapshot(value: &M) -> ModelSnapshot<M> {
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

    pub(super) fn finish(&self, value: &mut M) -> <M::Children as ModelChildren<M>>::Pending {
        self.children.finish(value)
    }

    pub(super) fn publish(
        &self,
        before: ModelSnapshot<M>,
        after: ModelSnapshot<M>,
        change: M::Change,
        pending: <M::Children as ModelChildren<M>>::Pending,
    ) {
        M::publish(&self.sources, before.observed, after.observed);
        M::publish_derived(&self.derived_sources, before.derived, after.derived);
        M::publish_change(&self.watches, change);
        self.children
            .publish(before.children, after.children, pending);
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
