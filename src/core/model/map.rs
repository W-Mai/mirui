use super::{CollectionError, Model, ModelItemId, ModelList, OwnedModel};
use alloc::vec::Vec;
use core::{
    borrow::Borrow,
    ops::{Deref, DerefMut},
};

mod load;
#[cfg(feature = "persistence")]
mod persistence;
mod reconcile;
mod registered;

struct KeyEntry<K> {
    key: K,
    slot: usize,
}

/// Ordered business keys associated with independently observable Model members.
///
/// Borrowed lookup does not clone keys. Inserting an existing key replaces its data while preserving the member identity; removing and reinserting a key creates a new identity. Capacity is prepared explicitly, and ordinary edits never grow storage. Iteration follows key order.
#[crate::model]
pub struct ModelMap<K: Ord + 'static, M: Model> {
    keys: Vec<KeyEntry<K>>,
    members: model!(owned ModelList<M>),
}

#[crate::model]
impl<K: Ord + 'static, M: Model> ModelMap<K, M> {
    pub fn new() -> Self {
        Self {
            keys: Vec::new(),
            members: ModelList::new().into(),
        }
    }
}

impl<K: Ord + 'static, M: Model> Default for ModelMap<K, M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Ord + 'static, M: Model> ModelMap<K, M> {
    pub fn with_capacity(capacity: usize) -> Self {
        let mut map = Self::new();
        map.reserve(capacity).expect("model map capacity");
        map
    }

    pub fn bounded(limit: usize) -> Self {
        Self {
            keys: Vec::with_capacity(limit),
            members: ModelList::bounded(limit).into(),
        }
    }

    pub fn len(&self) -> usize {
        self.members.len()
    }
    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }
    pub fn capacity(&self) -> usize {
        self.members.capacity()
    }
    pub fn limit(&self) -> Option<usize> {
        self.members.limit()
    }

    pub fn reserve(&mut self, additional: usize) -> Result<(), CollectionError> {
        let required = self
            .len()
            .checked_add(additional)
            .ok_or(CollectionError::Limit)?;
        if self.limit().is_some_and(|limit| required > limit) {
            return Err(CollectionError::Limit);
        }
        self.keys
            .try_reserve(additional)
            .map_err(|_| CollectionError::Allocation)?;
        self.members.reserve(additional)
    }

    fn position<Q: Ord + ?Sized>(&self, key: &Q) -> Result<usize, usize>
    where
        K: Borrow<Q>,
    {
        self.keys
            .binary_search_by(|entry| entry.key.borrow().cmp(key))
    }

    pub fn get<Q: Ord + ?Sized>(&self, key: &Q) -> Option<&M::Owned>
    where
        K: Borrow<Q>,
    {
        self.position(key)
            .ok()
            .and_then(|index| self.members.get(index))
    }

    pub fn get_mut<Q: Ord + ?Sized>(&mut self, key: &Q) -> Option<&mut M::Owned>
    where
        K: Borrow<Q>,
    {
        let index = self.position(key).ok()?;
        self.members.get_mut(index)
    }

    pub fn contains_key<Q: Ord + ?Sized>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
    {
        self.position(key).is_ok()
    }

    pub fn get_by_id(&self, id: ModelItemId) -> Option<&M::Owned> {
        self.members.get_by_id(id)
    }
    pub fn get_by_id_mut(&mut self, id: ModelItemId) -> Option<&mut M::Owned> {
        self.members.get_by_id_mut(id)
    }

    pub fn values(&self) -> impl DoubleEndedIterator<Item = &M::Owned> + ExactSizeIterator {
        self.members.iter()
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = (&K, &M::Owned)> + ExactSizeIterator {
        self.keys
            .iter()
            .zip(self.members.iter())
            .map(|(entry, value)| (&entry.key, value))
    }

    pub fn iter_mut(
        &mut self,
    ) -> impl DoubleEndedIterator<Item = (&K, &mut M::Owned)> + ExactSizeIterator {
        self.keys
            .iter()
            .zip(self.members.iter_mut())
            .map(|(entry, value)| (&entry.key, value))
    }

    pub fn entries(
        &self,
    ) -> impl DoubleEndedIterator<Item = (ModelItemId, &M::Owned)> + ExactSizeIterator {
        self.members.entries()
    }

    pub fn for_each(&self, mut visit: impl FnMut(&K, &M::Owned)) {
        for (key, value) in self.iter() {
            visit(key, value);
        }
    }

    /// Return the member identity and any replaced data. Failure preserves both inputs.
    pub fn try_insert(
        &mut self,
        key: K,
        value: M,
    ) -> Result<(ModelItemId, Option<M>), (CollectionError, K, M)> {
        match self.position(&key) {
            Ok(index) => {
                let id = self.members.entries().nth(index).expect("map member").0;
                self.members
                    .replace(id, value)
                    .map(|old| (id, Some(old)))
                    .map_err(|(error, value)| (error, key, value))
            }
            Err(index) => match self.members.try_insert(index, value) {
                Ok(id) => {
                    self.keys.insert(index, KeyEntry { key, slot: id.slot });
                    Ok((id, None))
                }
                Err((error, value)) => Err((error, key, value)),
            },
        }
    }

    pub fn remove<Q: Ord + ?Sized>(&mut self, key: &Q) -> Option<M>
    where
        K: Borrow<Q>,
    {
        let index = self.position(key).ok()?;
        let id = self.members.entries().nth(index).expect("map member").0;
        let value = self.members.remove(id);
        self.keys.remove(index);
        value
    }

    pub fn retain(&mut self, mut keep: impl FnMut(&K, &M::Owned) -> bool) {
        struct Reindex<'a, K: Ord + 'static, M: Model>(&'a mut ModelMap<K, M>);
        impl<K: Ord + 'static, M: Model> Drop for Reindex<'_, K, M> {
            fn drop(&mut self) {
                self.0
                    .keys
                    .retain(|entry| self.0.members.contains_slot(entry.slot));
            }
        }
        let guard = Reindex(self);
        let mut index = 0;
        guard.0.members.retain(|value| {
            let retain = keep(&guard.0.keys[index].key, value);
            index += 1;
            retain
        });
    }

    pub fn clear(&mut self) {
        self.retain(|_, _| false);
    }

    /// Move entries into key order, rejecting duplicate keys and returning the complete input batch.
    pub fn from_entries(
        entries: impl IntoIterator<Item = (K, M)>,
    ) -> Result<Self, (CollectionError, Vec<(K, M)>)> {
        let mut entries: Vec<_> = entries.into_iter().collect();
        entries.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        if entries.windows(2).any(|pair| pair[0].0 == pair[1].0) {
            return Err((CollectionError::DuplicateKey, entries));
        }
        let mut map = Self::with_capacity(entries.len());
        for (key, value) in entries {
            assert!(map.try_insert(key, value).is_ok());
        }
        Ok(map)
    }
}

impl<K: Ord + 'static, M: Model> Deref for ModelMapOwned<K, M> {
    type Target = ModelMap<K, M>;
    fn deref(&self) -> &Self::Target {
        &self.storage().value
    }
}

impl<K: Ord + 'static, M: Model> DerefMut for ModelMapOwned<K, M> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.storage_mut().value
    }
}

#[cfg(all(test, feature = "std"))]
mod tests;
