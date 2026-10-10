use super::{
    BindingOwner, Model, ModelAccess, ModelMetadata, ModelPath, ModelRoot, OwnedStorage, OwnerState,
};
use alloc::rc::{Rc, Weak};
use core::cell::RefCell;

/// Declared child identities and their retained transaction state.
#[doc(hidden)]
pub trait ModelChildren<M: Model>: Sized {
    fn register<R: Model, P: ModelPath<R, Target = M>>(
        root: &Weak<ModelRoot<R>>,
        owner: BindingOwner,
        path: P,
        value: &mut Option<&mut M>,
    ) -> Self;
    fn begin(&self, value: &M);
    fn extract(&self, value: &mut M);
    fn finish(&self, value: &mut M, reset: bool);
    fn capture_after(&self, value: &M);
    fn publish(&self);
    fn deliver(&self);
    fn discard(&self);
    fn retire(&self);
}

impl<M: Model> ModelChildren<M> for () {
    fn register<R: Model, P: ModelPath<R, Target = M>>(
        _: &Weak<ModelRoot<R>>,
        _: BindingOwner,
        _: P,
        _: &mut Option<&mut M>,
    ) {
    }
    fn begin(&self, _: &M) {}
    fn extract(&self, _: &mut M) {}
    fn finish(&self, _: &mut M, _: bool) {}
    fn capture_after(&self, _: &M) {}
    fn publish(&self) {}
    fn deliver(&self) {}
    fn discard(&self) {}
    fn retire(&self) {}
}

struct ModelSnapshot<M: Model> {
    observed: M::Snapshot,
    derived: M::DerivedSnapshot,
}

impl<M: Model> ModelSnapshot<M> {
    fn capture(value: &M) -> Self {
        Self {
            observed: value.snapshot(),
            derived: value.derived_snapshot(),
        }
    }
}

pub(super) struct ModelTransaction<M: Model> {
    before: Option<ModelSnapshot<M>>,
    after: Option<ModelSnapshot<M>>,
    pending: M::PendingWatches,
    events: Option<M::Events>,
}

impl<M: Model> ModelTransaction<M> {
    fn new() -> Self {
        Self {
            before: None,
            after: None,
            pending: M::pending_watches(),
            events: None,
        }
    }
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
        mut storage: Option<&mut OwnedStorage<M>>,
    ) -> Self {
        let node = Self {
            metadata: ModelMetadata::register::<R, P>(
                root,
                owner,
                path,
                storage.as_deref_mut().map(|storage| &mut storage.value),
            ),
            access: Rc::new(super::projection::Projection::<R, P> {
                root: root.clone(),
                path,
            }),
        };
        if let Some(storage) = storage {
            storage.identity = Some(Rc::downgrade(&node.access));
            storage.pending = M::pending_watches();
        }
        node
    }

    pub fn metadata(&self) -> &ModelMetadata<M> {
        &self.metadata
    }

    pub fn handle(&self) -> M::Handle {
        M::handle(Rc::downgrade(&self.access))
    }

    pub fn begin(&self, storage: &OwnedStorage<M>) {
        self.metadata.begin(&storage.value);
    }

    pub fn extract(&self, storage: &mut OwnedStorage<M>) {
        self.metadata.extract(&mut storage.value);
    }

    pub fn finish(&self, storage: &mut OwnedStorage<M>, reset: bool) {
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
            .finish(&mut storage.value, pending, reset || moved);
    }

    pub fn capture_after(&self, storage: &OwnedStorage<M>) {
        self.metadata.capture_after(&storage.value);
    }

    pub fn publish(&self) {
        self.metadata.publish();
    }

    pub fn deliver(&self) {
        self.metadata.deliver();
    }

    pub fn discard(&self) {
        self.metadata.discard();
    }

    pub fn retire(&self) {
        self.metadata.retire();
    }
}

impl<M: Model> ModelMetadata<M> {
    pub(super) fn register<R: Model, P: ModelPath<R, Target = M>>(
        root: &Weak<ModelRoot<R>>,
        owner: BindingOwner,
        path: P,
        value: Option<&mut M>,
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
            children: M::Children::register::<R, P>(root, owner, path, &mut { value }),
            transaction: RefCell::new(ModelTransaction::new()),
        }
    }

    #[doc(hidden)]
    pub fn children(&self) -> &M::Children {
        &self.children
    }

    pub(super) fn begin(&self, value: &M) {
        let before = ModelSnapshot::capture(value);
        self.transaction.borrow_mut().before = Some(before);
        self.children.begin(value);
    }

    pub(super) fn extract(&self, value: &mut M) {
        let events = value.take_events();
        self.transaction.borrow_mut().events = Some(events);
        self.children.extract(value);
    }

    pub(super) fn finish(&self, value: &mut M, mut pending: M::PendingWatches, reset: bool) {
        if reset {
            pending.as_mut().fill(true);
        }
        self.transaction.borrow_mut().pending = pending;
        self.children.finish(value, reset);
    }

    pub(super) fn capture_after(&self, value: &M) {
        let after = ModelSnapshot::capture(value);
        self.transaction.borrow_mut().after = Some(after);
        self.children.capture_after(value);
    }

    pub(super) fn publish(&self) {
        let (before, after, pending) = {
            let mut transaction = self.transaction.borrow_mut();
            (
                transaction
                    .before
                    .take()
                    .expect("model transaction has no before snapshot"),
                transaction
                    .after
                    .take()
                    .expect("model transaction has no after snapshot"),
                core::mem::replace(&mut transaction.pending, M::pending_watches()),
            )
        };
        M::publish(&self.sources, before.observed, after.observed);
        M::publish_derived(&self.derived_sources, before.derived, after.derived);
        for (watch, changed) in self.watches.as_ref().iter().zip(pending.as_ref()) {
            if *changed {
                watch.publish();
            }
        }
        self.children.publish();
    }

    pub(super) fn deliver(&self) {
        let events = self.transaction.borrow_mut().events.take();
        if let Some(events) = events {
            M::deliver_events(&self.routes, events);
        }
        self.children.deliver();
    }

    pub(super) fn discard(&self) {
        let discarded = {
            let mut transaction = self.transaction.borrow_mut();
            core::mem::replace(&mut *transaction, ModelTransaction::new())
        };
        drop(discarded);
        self.children.discard();
    }

    pub(super) fn retire(&self) {
        for source in self
            .sources
            .as_ref()
            .iter()
            .chain(self.derived_sources.as_ref())
        {
            source.retire();
        }
        for watch in self.watches.as_ref() {
            watch.retire();
        }
        self.children.retire();
    }
}

#[cfg(all(test, feature = "std"))]
mod tests;
