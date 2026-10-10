use super::{
    BindingOwner, ChildNode, Model, ModelAccess, ModelMetadata, ModelOwnerError, ModelRoot,
    ModelUpdate, OwnedStorage, OwnerState,
};
use alloc::rc::Weak;
use core::marker::PhantomData;

/// A statically named child field and its registered identity.
#[doc(hidden)]
pub trait ModelChildField<M: Model>: 'static {
    type Child: Model;

    fn storage(value: &M) -> &OwnedStorage<Self::Child>;
    fn storage_mut(value: &mut M) -> &mut OwnedStorage<Self::Child>;
    fn node(metadata: &ModelMetadata<M>) -> &ChildNode<Self::Child>;
}

/// Safe projection of data and metadata from one registered root.
#[doc(hidden)]
pub trait ModelPath<R: Model>: Copy + 'static {
    type Target: Model;

    fn read<'a>(&self, value: &'a R) -> &'a Self::Target;
    fn write<'a>(&self, value: &'a mut R) -> &'a mut Self::Target;
    fn metadata<'a>(&self, root: &'a ModelMetadata<R>) -> &'a ModelMetadata<Self::Target>;
    fn update(
        &self,
        value: &mut R,
        pending: &mut R::PendingWatches,
        update: &mut ModelUpdate<'_, Self::Target>,
    ) -> bool;
}

#[derive(Clone, Copy)]
pub(super) struct RootPath;

impl<M: Model> ModelPath<M> for RootPath {
    type Target = M;

    fn read<'a>(&self, value: &'a M) -> &'a M {
        value
    }
    fn write<'a>(&self, value: &'a mut M) -> &'a mut M {
        value
    }
    fn metadata<'a>(&self, root: &'a ModelMetadata<M>) -> &'a ModelMetadata<M> {
        root
    }

    fn update(
        &self,
        value: &mut M,
        pending: &mut M::PendingWatches,
        update: &mut ModelUpdate<'_, M>,
    ) -> bool {
        let Some(change) = update(value) else {
            return false;
        };
        M::record_change(pending, change);
        true
    }
}

/// Composition of a model path with one declared child field.
#[doc(hidden)]
pub struct ModelPathThen<P, F> {
    parent: P,
    field: PhantomData<fn() -> F>,
}

impl<P, F> ModelPathThen<P, F> {
    pub const fn new(parent: P) -> Self {
        Self {
            parent,
            field: PhantomData,
        }
    }
}

impl<P: Copy, F> Copy for ModelPathThen<P, F> {}

impl<P: Copy, F> Clone for ModelPathThen<P, F> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<R: Model, P: ModelPath<R>, F: ModelChildField<P::Target>> ModelPath<R>
    for ModelPathThen<P, F>
{
    type Target = F::Child;

    fn read<'a>(&self, value: &'a R) -> &'a Self::Target {
        &F::storage(self.parent.read(value)).value
    }

    fn write<'a>(&self, value: &'a mut R) -> &'a mut Self::Target {
        &mut F::storage_mut(self.parent.write(value)).value
    }

    fn metadata<'a>(&self, root: &'a ModelMetadata<R>) -> &'a ModelMetadata<Self::Target> {
        F::node(self.parent.metadata(root)).metadata()
    }

    fn update(
        &self,
        value: &mut R,
        _: &mut R::PendingWatches,
        update: &mut ModelUpdate<'_, Self::Target>,
    ) -> bool {
        let storage = F::storage_mut(self.parent.write(value));
        let Some(change) = update(&mut storage.value) else {
            return false;
        };
        F::Child::record_change(&mut storage.pending, change);
        true
    }
}

pub(super) struct Projection<R: Model, P: ModelPath<R>> {
    pub(super) root: Weak<ModelRoot<R>>,
    pub(super) path: P,
}

impl<R: Model, P: ModelPath<R>> Projection<R, P> {
    fn root(&self) -> alloc::rc::Rc<ModelRoot<R>> {
        self.root
            .upgrade()
            .expect("model registration is no longer alive")
    }
}

impl<R: Model, P: ModelPath<R>> ModelAccess<P::Target> for Projection<R, P> {
    fn generation(&self) -> Option<u64> {
        let root = self.root.upgrade()?;
        root.generation()?;
        self.path.metadata(&root.metadata).lifetime.current()
    }

    fn binding_owner(&self) -> BindingOwner {
        if self.generation().is_none() {
            return BindingOwner(OwnerState::Invalid(ModelOwnerError::RegistrationEnded));
        }
        self.root.upgrade().map_or(
            BindingOwner(OwnerState::Invalid(ModelOwnerError::RegistrationEnded)),
            |root| root.binding_owner(),
        )
    }

    fn read(&self, read: &mut dyn FnMut(&P::Target, &ModelMetadata<P::Target>)) {
        self.root().read(&mut |value, metadata| {
            read(self.path.read(value), self.path.metadata(metadata));
        });
    }

    fn update(&self, update: &mut ModelUpdate<'_, P::Target>) {
        self.root().update_path(&self.path, update);
    }

    fn metadata(&self, read: &mut dyn FnMut(&ModelMetadata<P::Target>)) {
        self.root()
            .metadata(&mut |metadata| read(self.path.metadata(metadata)));
    }
}
