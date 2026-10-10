use super::{CollectionError, ModelSet, ModelSetHandle};
use crate::core::model::{CollectionLoadError, ModelHandle, update_model};
use alloc::vec::Vec;
use core::borrow::Borrow;

impl<K: Ord + 'static> ModelSetHandle<K> {
    fn observed<R>(&self, read: impl FnOnce(&ModelSet<K>) -> R) -> R {
        self.version();
        self.read(read)
    }
    pub fn len(&self) -> usize {
        self.observed(ModelSet::len)
    }
    pub fn is_empty(&self) -> bool {
        self.observed(ModelSet::is_empty)
    }
    pub fn capacity(&self) -> usize {
        self.read(ModelSet::capacity)
    }
    pub fn limit(&self) -> Option<usize> {
        self.read(ModelSet::limit)
    }
    pub fn contains<Q: Ord + ?Sized>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
    {
        self.observed(|set| set.contains(key))
    }
    pub fn for_each(&self, visit: impl FnMut(&K)) {
        self.observed(|set| set.for_each(visit));
    }
    pub fn to_vec(&self) -> Vec<K>
    where
        K: Clone,
    {
        self.observed(ModelSet::to_vec)
    }
    pub fn reserve(&self, additional: usize) -> Result<(), CollectionError> {
        update_model(
            self,
            |set| set.reserve(additional),
            |result| result.as_ref().ok().copied(),
        )
    }

    pub fn load<I: IntoIterator<Item = K>>(
        &self,
        items: I,
    ) -> Result<(), CollectionLoadError<I::IntoIter>> {
        update_model(
            self,
            |set| set.load(items),
            |result| result.as_ref().ok().copied(),
        )
    }

    pub fn reconcile<I: IntoIterator<Item = K>>(
        &self,
        items: I,
    ) -> Result<(), CollectionLoadError<I::IntoIter>> {
        update_model(
            self,
            |set| set.reconcile(items),
            |result| result.as_ref().ok().copied(),
        )
    }
    pub fn try_insert(&self, value: K) -> Result<bool, (CollectionError, K)> {
        update_model(
            self,
            |set| set.try_insert(value),
            |result| result.as_ref().ok().map(|_| ()),
        )
    }
    pub fn remove<Q: Ord + ?Sized>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
    {
        update_model(self, |set| set.remove(key), |_| Some(()))
    }
    pub fn clear(&self) {
        update_model(self, ModelSet::clear, |_| Some(()));
    }
    pub fn retain(&self, keep: impl FnMut(&K) -> bool) {
        update_model(self, |set| set.retain(keep), |_| Some(()));
    }
}
