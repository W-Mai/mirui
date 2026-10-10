use super::{CollectionError, OwnedModel, collection::next_collection_id};
use alloc::vec::Vec;
use core::{
    borrow::Borrow,
    ops::{Deref, DerefMut},
};

mod registered;

/// Observable membership of sorted, immutable values.
///
/// Values do not become child Models. Lookup borrows keys, duplicate insertion does not publish a change, and ordinary edits never expand prepared capacity. `reserve` explicitly prepares space; `bounded` also imposes a hard limit.
#[crate::model]
pub struct ModelSet<K: Ord + 'static> {
    values: Vec<K>,
    prepared: usize,
    limit: Option<usize>,
    #[observe]
    version: (u64, u64),
}

#[crate::model]
impl<K: Ord + 'static> ModelSet<K> {
    pub fn new() -> Self {
        Self {
            values: Vec::new(),
            prepared: 0,
            limit: None,
            version: (next_collection_id(), 0),
        }
    }
}

impl<K: Ord + 'static> Default for ModelSet<K> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Ord + 'static> ModelSet<K> {
    pub fn with_capacity(capacity: usize) -> Self {
        let mut set = Self::new();
        set.reserve(capacity).expect("model set capacity");
        set
    }
    pub fn bounded(limit: usize) -> Self {
        let mut set = Self::with_capacity(limit);
        set.limit = Some(limit);
        set
    }
    pub fn len(&self) -> usize {
        self.values.len()
    }
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
    pub fn capacity(&self) -> usize {
        self.prepared
    }
    pub fn limit(&self) -> Option<usize> {
        self.limit
    }

    pub fn reserve(&mut self, additional: usize) -> Result<(), CollectionError> {
        let required = self
            .len()
            .checked_add(additional)
            .ok_or(CollectionError::Limit)?;
        if self.limit.is_some_and(|limit| required > limit) {
            return Err(CollectionError::Limit);
        }
        if required > self.prepared {
            self.values
                .try_reserve(additional)
                .map_err(|_| CollectionError::Allocation)?;
            self.prepared = required;
        }
        Ok(())
    }

    fn position<Q: Ord + ?Sized>(&self, key: &Q) -> Result<usize, usize>
    where
        K: Borrow<Q>,
    {
        self.values
            .binary_search_by(|value| value.borrow().cmp(key))
    }

    pub fn contains<Q: Ord + ?Sized>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
    {
        self.position(key).is_ok()
    }

    pub fn try_insert(&mut self, value: K) -> Result<bool, (CollectionError, K)> {
        let index = match self.position(&value) {
            Ok(_) => return Ok(false),
            Err(index) => index,
        };
        if self.len() == self.prepared {
            return Err((CollectionError::Capacity, value));
        }
        self.values.insert(index, value);
        self.changed();
        Ok(true)
    }

    pub fn remove<Q: Ord + ?Sized>(&mut self, key: &Q) -> bool
    where
        K: Borrow<Q>,
    {
        let Ok(index) = self.position(key) else {
            return false;
        };
        let removed = self.values.remove(index);
        self.changed();
        drop(removed);
        true
    }

    pub fn retain(&mut self, keep: impl FnMut(&K) -> bool) {
        struct Publish<'a, K: Ord + 'static>(&'a mut ModelSet<K>, usize);
        impl<K: Ord + 'static> Drop for Publish<'_, K> {
            fn drop(&mut self) {
                if self.0.len() != self.1 {
                    self.0.changed();
                }
            }
        }
        let len = self.len();
        let guard = Publish(self, len);
        guard.0.values.retain(keep);
    }

    pub fn clear(&mut self) {
        self.retain(|_| false);
    }
    pub fn for_each(&self, mut visit: impl FnMut(&K)) {
        for value in &self.values {
            visit(value);
        }
    }
    pub fn to_vec(&self) -> Vec<K>
    where
        K: Clone,
    {
        self.values.clone()
    }
    fn changed(&mut self) {
        self.version.1 = self
            .version
            .1
            .checked_add(1)
            .expect("model set revision exhausted");
    }
}

impl<K: Ord + 'static> FromIterator<K> for ModelSet<K> {
    fn from_iter<I: IntoIterator<Item = K>>(items: I) -> Self {
        let items = items.into_iter();
        let mut values = Vec::with_capacity(items.size_hint().0);
        for item in items {
            values.push(item);
        }
        let prepared = values.len();
        values.sort_unstable();
        values.dedup();
        Self {
            values,
            prepared,
            limit: None,
            version: (next_collection_id(), 0),
        }
    }
}

impl<K: Ord + 'static> Deref for ModelSetOwned<K> {
    type Target = ModelSet<K>;
    fn deref(&self) -> &Self::Target {
        &self.storage().value
    }
}
impl<K: Ord + 'static> DerefMut for ModelSetOwned<K> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.storage_mut().value
    }
}
impl<K: Ord + 'static> FromIterator<K> for ModelSetOwned<K> {
    fn from_iter<I: IntoIterator<Item = K>>(items: I) -> Self {
        ModelSet::from_iter(items).into()
    }
}

#[cfg(all(test, feature = "std"))]
mod tests;
