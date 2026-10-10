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
pub trait ModelPath<R: Model>: 'static {
    type Target: Model;

    fn read(value: &R) -> &Self::Target;
    fn write(value: &mut R) -> &mut Self::Target;
    fn metadata(root: &ModelMetadata<R>) -> &ModelMetadata<Self::Target>;
    fn update(
        value: &mut R,
        pending: &mut R::PendingWatches,
        update: &mut ModelUpdate<'_, Self::Target>,
    ) -> bool;
}

pub(super) struct RootPath;

impl<M: Model> ModelPath<M> for RootPath {
    type Target = M;

    fn read(value: &M) -> &M {
        value
    }
    fn write(value: &mut M) -> &mut M {
        value
    }
    fn metadata(root: &ModelMetadata<M>) -> &ModelMetadata<M> {
        root
    }

    fn update(
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
pub struct ModelPathThen<P, F>(PhantomData<fn() -> (P, F)>);

impl<R: Model, P: ModelPath<R>, F: ModelChildField<P::Target>> ModelPath<R>
    for ModelPathThen<P, F>
{
    type Target = F::Child;

    fn read(value: &R) -> &Self::Target {
        &F::storage(P::read(value)).value
    }

    fn write(value: &mut R) -> &mut Self::Target {
        &mut F::storage_mut(P::write(value)).value
    }

    fn metadata(root: &ModelMetadata<R>) -> &ModelMetadata<Self::Target> {
        F::node(P::metadata(root)).metadata()
    }

    fn update(
        value: &mut R,
        _: &mut R::PendingWatches,
        update: &mut ModelUpdate<'_, Self::Target>,
    ) -> bool {
        let storage = F::storage_mut(P::write(value));
        let Some(change) = update(&mut storage.value) else {
            return false;
        };
        F::Child::record_change(&mut storage.pending, change);
        true
    }
}

pub(super) struct Projection<R: Model, P: ModelPath<R>> {
    pub(super) root: Weak<ModelRoot<R>>,
    pub(super) path: PhantomData<fn() -> P>,
}

impl<R: Model, P: ModelPath<R>> Projection<R, P> {
    fn root(&self) -> alloc::rc::Rc<ModelRoot<R>> {
        self.root
            .upgrade()
            .expect("model registration is no longer alive")
    }
}

impl<R: Model, P: ModelPath<R>> ModelAccess<P::Target> for Projection<R, P> {
    fn binding_owner(&self) -> BindingOwner {
        self.root.upgrade().map_or(
            BindingOwner(OwnerState::Invalid(ModelOwnerError::RegistrationEnded)),
            |root| root.binding_owner(),
        )
    }

    fn read(&self, read: &mut dyn FnMut(&P::Target, &ModelMetadata<P::Target>)) {
        self.root()
            .read(&mut |value, metadata| read(P::read(value), P::metadata(metadata)));
    }

    fn update(&self, update: &mut ModelUpdate<'_, P::Target>) {
        self.root().update_path::<P>(update);
    }

    fn metadata(&self, read: &mut dyn FnMut(&ModelMetadata<P::Target>)) {
        self.root()
            .metadata(&mut |metadata| read(P::metadata(metadata)));
    }
}
