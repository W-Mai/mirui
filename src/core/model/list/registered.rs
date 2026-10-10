use super::{
    CollectionError, ModelItemId, ModelList,
    state::{ListChildren, ListState},
};
use crate::core::model::{
    BindType, BindingOwner, Model, ModelHandle, ModelMethods, ModelRef, ModelWatch, OwnedModel,
    OwnedStorage, SharedValue, read_model, update_model,
};
use crate::core::reactive::ModelSource;
use alloc::{rc::Rc, vec::Vec};
use core::ops::{Deref, DerefMut};

/// Inline representation of an owned model list.
#[doc(hidden)]
pub struct ModelListOwned<M: Model>(OwnedStorage<ModelList<M>>);

impl<M: Model> From<ModelList<M>> for ModelListOwned<M> {
    fn from(value: ModelList<M>) -> Self {
        Self(OwnedStorage::new(value))
    }
}
impl<M: Model> From<Vec<M>> for ModelListOwned<M> {
    fn from(value: Vec<M>) -> Self {
        ModelList::from(value).into()
    }
}
impl<M: Model, const N: usize> From<[M; N]> for ModelListOwned<M> {
    fn from(value: [M; N]) -> Self {
        ModelList::from(value).into()
    }
}
impl<M: Model> FromIterator<M> for ModelListOwned<M> {
    fn from_iter<I: IntoIterator<Item = M>>(items: I) -> Self {
        ModelList::from_iter(items).into()
    }
}
impl<M: Model> OwnedModel for ModelListOwned<M> {
    type Data = ModelList<M>;
    fn storage(&self) -> &OwnedStorage<Self::Data> {
        &self.0
    }
    fn storage_mut(&mut self) -> &mut OwnedStorage<Self::Data> {
        &mut self.0
    }
    fn into_data(self) -> Self::Data {
        self.0.into_data()
    }
}
impl<M: Model> Deref for ModelListOwned<M> {
    type Target = ModelList<M>;
    fn deref(&self) -> &Self::Target {
        &self.0.value
    }
}
impl<M: Model> DerefMut for ModelListOwned<M> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0.value
    }
}

impl<M: Model> ModelMethods for ModelList<M> {
    type DerivedSnapshot = ();
    type DerivedSources = [ModelSource; 0];
    type Events = ();
    type Routes = ();
    fn derived_snapshot(&self) {}
    fn derived_sources() -> Self::DerivedSources {
        []
    }
    fn publish_derived(_: &Self::DerivedSources, _: (), _: ()) {}
    fn take_events(&mut self) {}
    fn routes() {}
    fn retire_routes(_: &()) {}
    fn deliver_events(_: &(), _: ()) {}
}

impl<M: Model> Model for ModelList<M> {
    type Handle = ModelListHandle<M>;
    type Owned = ModelListOwned<M>;
    type Snapshot = (u64, u64);
    type Sources = [ModelSource; 1];
    type Change = ();
    type Watches = [ModelWatch; 0];
    type PendingWatches = [bool; 0];
    type Children = ListChildren<M>;
    fn handle(access: ModelRef<Self>) -> Self::Handle {
        ModelListHandle { access }
    }
    fn snapshot(&self) -> Self::Snapshot {
        (self.identity, self.revision)
    }
    fn sources() -> Self::Sources {
        [ModelSource::new()]
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
    fn record_change(_: &mut Self::PendingWatches, _: ()) {}
}

/// Registered access to an ordered model collection.
#[doc(hidden)]
pub struct ModelListHandle<M: Model> {
    access: ModelRef<ModelList<M>>,
}

impl<M: Model> Clone for ModelListHandle<M> {
    fn clone(&self) -> Self {
        Self {
            access: self.access.clone(),
        }
    }
}
impl<M: Model> ModelHandle for ModelListHandle<M> {
    type Data = ModelList<M>;
    fn access(&self) -> &ModelRef<Self::Data> {
        &self.access
    }
}
impl<M: Model> SharedValue for ModelListHandle<M> {
    fn callback_ready(&self) -> bool {
        ModelHandle::callback_ready(self)
    }
    fn binding_owner(&self) -> BindingOwner {
        BindingOwner::registered(self)
    }
}
impl<M: Model> BindType for ModelList<M> {
    type Shared = ModelListHandle<M>;
}
impl<M: Model> BindType for ModelListHandle<M> {
    type Shared = Self;
}

impl<M: Model> ModelListHandle<M> {
    pub fn len(&self) -> usize {
        self.read_observed(0, ModelList::len)
    }
    pub fn is_empty(&self) -> bool {
        self.read_observed(0, ModelList::is_empty)
    }
    pub fn capacity(&self) -> usize {
        self.read(ModelList::capacity)
    }
    pub fn limit(&self) -> Option<usize> {
        self.read(ModelList::limit)
    }

    pub fn get(&self, index: usize) -> Option<M::Handle> {
        read_model(self, |value, metadata| {
            metadata.sources[0].track();
            value
                .entries
                .get(index)
                .map(|entry| metadata.children().0.node(entry.slot).handle())
        })
    }

    pub fn get_by_id(&self, id: ModelItemId) -> Option<M::Handle> {
        read_model(self, |value, metadata| {
            metadata.sources[0].track();
            value
                .position(id)
                .map(|_| metadata.children().0.node(id.slot).handle())
        })
    }

    pub fn entries(&self) -> ModelListEntries<M> {
        read_model(self, |value, metadata| {
            metadata.sources[0].track();
            let state = metadata.children().0.clone();
            state.leases.set(
                state
                    .leases
                    .get()
                    .checked_add(1)
                    .expect("model list iterator limit"),
            );
            ModelListEntries {
                list: self.clone(),
                state,
                front: 0,
                back: value.len(),
            }
        })
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = M::Handle> + ExactSizeIterator {
        self.entries().map(|(_, item)| item)
    }

    fn available(&self) -> Result<(), CollectionError> {
        read_model(self, |_, metadata| {
            if crate::core::model::ModelChildren::is_writable(metadata.children()) {
                Ok(())
            } else {
                Err(CollectionError::Iterating)
            }
        })
    }

    pub fn try_push(&self, value: M) -> Result<ModelItemId, (CollectionError, M)> {
        if let Err(error) = self.available() {
            return Err((error, value));
        }
        update_model(
            self,
            |list| list.try_push(value),
            |result| result.as_ref().ok().map(|_| ()),
        )
    }
    pub fn try_insert(&self, index: usize, value: M) -> Result<ModelItemId, (CollectionError, M)> {
        if let Err(error) = self.available() {
            return Err((error, value));
        }
        update_model(
            self,
            |list| list.try_insert(index, value),
            |result| result.as_ref().ok().map(|_| ()),
        )
    }
    pub fn remove(&self, id: ModelItemId) -> Result<Option<M>, CollectionError> {
        self.available()?;
        Ok(update_model(self, |list| list.remove(id), |_| Some(())))
    }
    pub fn replace(&self, id: ModelItemId, value: M) -> Result<M, (CollectionError, M)> {
        if let Err(error) = self.available() {
            return Err((error, value));
        }
        update_model(
            self,
            |list| list.replace(id, value),
            |result| result.as_ref().ok().map(|_| ()),
        )
    }
    pub fn clear(&self) -> Result<(), CollectionError> {
        self.available()?;
        update_model(self, ModelList::clear, |_| Some(()));
        Ok(())
    }
    pub fn retain(&self, keep: impl FnMut(&M::Owned) -> bool) -> Result<(), CollectionError> {
        self.available()?;
        update_model(self, |list| list.retain(keep), |_| Some(()));
        Ok(())
    }
    pub fn swap(&self, a: usize, b: usize) -> Result<(), CollectionError> {
        self.available()?;
        update_model(
            self,
            |list| list.swap(a, b),
            |result| result.as_ref().ok().copied(),
        )
    }
    pub fn move_to(&self, id: ModelItemId, index: usize) -> Result<(), CollectionError> {
        self.available()?;
        update_model(
            self,
            |list| list.move_to(id, index),
            |result| result.as_ref().ok().copied(),
        )
    }
    pub fn rotate_left(&self, amount: usize) -> Result<(), CollectionError> {
        self.available()?;
        update_model(
            self,
            |list| list.rotate_left(amount),
            |result| result.as_ref().ok().copied(),
        )
    }
    pub fn reserve(&self, additional: usize) -> Result<(), CollectionError> {
        self.available()?;
        update_model(
            self,
            |list| list.reserve(additional),
            |result| result.as_ref().ok().copied(),
        )
    }
}

/// A structural lease that permits updates to the yielded model items.
pub struct ModelListEntries<M: Model> {
    list: ModelListHandle<M>,
    state: Rc<ListState<M>>,
    front: usize,
    back: usize,
}

impl<M: Model> ModelListEntries<M> {
    fn at(&self, index: usize) -> (ModelItemId, M::Handle) {
        read_model(&self.list, |list, metadata| {
            let entry = &list.entries[index];
            (
                list.id(entry.slot),
                metadata.children().0.node(entry.slot).handle(),
            )
        })
    }
}
impl<M: Model> Iterator for ModelListEntries<M> {
    type Item = (ModelItemId, M::Handle);
    fn next(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        let item = self.at(self.front);
        self.front += 1;
        Some(item)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.back - self.front;
        (len, Some(len))
    }
}
impl<M: Model> DoubleEndedIterator for ModelListEntries<M> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        let index = self.back - 1;
        let item = self.at(index);
        self.back = index;
        Some(item)
    }
}
impl<M: Model> ExactSizeIterator for ModelListEntries<M> {}
impl<M: Model> core::iter::FusedIterator for ModelListEntries<M> {}
impl<M: Model> Drop for ModelListEntries<M> {
    fn drop(&mut self) {
        self.state.leases.set(self.state.leases.get() - 1);
    }
}
