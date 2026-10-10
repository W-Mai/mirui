use super::{CollectionError, Model, ModelItemId, ModelMap, ModelMapHandle};
use crate::core::model::{
    CollectionLoadError, EffectRoute, ItemEffectSource, ModelHandle, ModelItemEffect, ModelList,
    ModelListEntries, ModelMetadata, Produces, update_model,
};
use core::borrow::Borrow;

impl<K: Ord + 'static, M: Model + ItemEffectSource<E>, E> Produces<ModelItemEffect<E>>
    for ModelMap<K, M>
{
    fn route(metadata: &ModelMetadata<Self>) -> &EffectRoute<ModelItemEffect<E>> {
        <ModelList<M> as Produces<ModelItemEffect<E>>>::route(
            metadata.children().members.metadata(),
        )
    }
}

impl<K: Ord + 'static, M: Model> ModelMapHandle<K, M> {
    pub fn len(&self) -> usize {
        self.members().len()
    }
    pub fn is_empty(&self) -> bool {
        self.members().is_empty()
    }
    pub fn capacity(&self) -> usize {
        self.read(ModelMap::capacity)
    }
    pub fn limit(&self) -> Option<usize> {
        self.read(ModelMap::limit)
    }

    pub fn get<Q: Ord + ?Sized>(&self, key: &Q) -> Option<M::Handle>
    where
        K: Borrow<Q>,
    {
        let members = self.members();
        members.observe_structure();
        let index = self.read(|map| map.position(key).ok());
        index.and_then(|index| members.get(index))
    }

    pub fn contains_key<Q: Ord + ?Sized>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
    {
        self.get(key).is_some()
    }

    pub fn get_by_id(&self, id: ModelItemId) -> Option<M::Handle> {
        self.members().get_by_id(id)
    }

    pub fn entries(&self) -> ModelListEntries<M> {
        self.members().entries()
    }

    pub fn values(&self) -> impl DoubleEndedIterator<Item = M::Handle> + ExactSizeIterator {
        self.entries().map(|(_, member)| member)
    }

    pub fn for_each(&self, mut visit: impl FnMut(&K, M::Handle)) {
        let members = self.members();
        members.observe_structure();
        self.read(|map| {
            for (index, entry) in map.keys.iter().enumerate() {
                visit(&entry.key, members.get(index).expect("map member"));
            }
        });
    }

    fn available(&self) -> Result<(), CollectionError> {
        self.read(|_| ());
        if ModelHandle::callback_ready(self) {
            Ok(())
        } else {
            Err(CollectionError::Iterating)
        }
    }

    pub fn reserve(&self, additional: usize) -> Result<(), CollectionError> {
        self.available()?;
        update_model(
            self,
            |map| map.reserve(additional),
            |result| result.as_ref().ok().copied(),
        )
    }

    pub fn load<I: IntoIterator<Item = (K, M)>>(
        &self,
        items: I,
    ) -> Result<(), CollectionLoadError<I::IntoIter>> {
        if let Err(error) = self.available() {
            return Err(CollectionLoadError::untouched(error, items.into_iter()));
        }
        update_model(
            self,
            |map| map.load(items),
            |result| result.as_ref().ok().copied(),
        )
    }

    pub fn reconcile<I: IntoIterator<Item = (K, M)>>(
        &self,
        items: I,
        merge: impl FnMut(&mut M, M),
    ) -> Result<(), CollectionLoadError<I::IntoIter>> {
        if let Err(error) = self.available() {
            return Err(CollectionLoadError::untouched(error, items.into_iter()));
        }
        update_model(
            self,
            |map| map.reconcile(items, merge),
            |result| result.as_ref().ok().copied(),
        )
    }

    pub fn try_insert(
        &self,
        key: K,
        value: M,
    ) -> Result<(ModelItemId, Option<M>), (CollectionError, K, M)> {
        if let Err(error) = self.available() {
            return Err((error, key, value));
        }
        update_model(
            self,
            |map| map.try_insert(key, value),
            |result| result.as_ref().ok().map(|_| ()),
        )
    }

    pub fn remove<Q: Ord + ?Sized>(&self, key: &Q) -> Result<Option<M>, CollectionError>
    where
        K: Borrow<Q>,
    {
        self.available()?;
        Ok(update_model(self, |map| map.remove(key), |_| Some(())))
    }

    pub fn clear(&self) -> Result<(), CollectionError> {
        self.available()?;
        update_model(self, ModelMap::clear, |_| Some(()));
        Ok(())
    }

    pub fn retain(&self, keep: impl FnMut(&K, &M::Owned) -> bool) -> Result<(), CollectionError> {
        self.available()?;
        update_model(self, |map| map.retain(keep), |_| Some(()));
        Ok(())
    }
}
