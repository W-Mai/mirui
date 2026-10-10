use super::{Model, ModelRef};

/// Inline data and pending changes for a statically owned model field.
#[doc(hidden)]
pub struct OwnedStorage<M: Model> {
    pub(super) value: M,
    pub(super) pending: M::PendingWatches,
    pub(super) identity: Option<ModelRef<M>>,
}

impl<M: Model> OwnedStorage<M> {
    pub fn new(value: M) -> Self {
        Self {
            value,
            pending: M::pending_watches(),
            identity: None,
        }
    }

    pub fn into_data(self) -> M {
        self.value
    }

    pub fn read<R>(&self, read: impl FnOnce(&M) -> R) -> R {
        read(&self.value)
    }

    pub fn update(&mut self, update: impl FnOnce(&mut M) -> M::Change) -> M::Change {
        let change = update(&mut self.value);
        M::record_change(&mut self.pending, change);
        change
    }

    pub fn try_update<E>(
        &mut self,
        update: impl FnOnce(&mut M) -> Result<M::Change, E>,
    ) -> Result<M::Change, E> {
        let change = update(&mut self.value)?;
        M::record_change(&mut self.pending, change);
        Ok(change)
    }
}

/// Access implemented by each model's generated inline representation.
#[doc(hidden)]
pub trait OwnedModel {
    type Data: Model;

    fn storage(&self) -> &OwnedStorage<Self::Data>;
    fn storage_mut(&mut self) -> &mut OwnedStorage<Self::Data>;
    fn into_data(self) -> Self::Data;

    fn read<R>(&self, read: impl FnOnce(&Self::Data) -> R) -> R {
        self.storage().read(read)
    }

    fn update(
        &mut self,
        update: impl FnOnce(&mut Self::Data) -> <Self::Data as Model>::Change,
    ) -> <Self::Data as Model>::Change {
        self.storage_mut().update(update)
    }

    fn try_update<E>(
        &mut self,
        update: impl FnOnce(&mut Self::Data) -> Result<<Self::Data as Model>::Change, E>,
    ) -> Result<<Self::Data as Model>::Change, E> {
        self.storage_mut().try_update(update)
    }
}
