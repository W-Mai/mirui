use super::{CollectionError, Model, ModelItemId, OwnedModel, collection::next_collection_id};
use alloc::rc::Rc;
use alloc::vec::Vec;

mod load;
mod reconcile;
mod registered;
mod state;
pub use registered::{ModelListEntries, ModelListHandle, ModelListOwned};

struct Slot {
    generation: u64,
    position: Option<usize>,
}

struct Entry<T> {
    slot: usize,
    value: T,
}

/// Owned model data in list order, with explicitly prepared capacity.
///
/// `try_push` and `try_insert` retain their input on failure and never grow storage. `reserve` prepares additional positions; `bounded` also sets a hard limit. Reordering and replacement preserve item IDs. Removal invalidates an ID even when a later insertion reuses its slot. Iteration follows the current order.
///
/// Declare an owned field with `items: model!(owned ModelList<Item>)`. The registered field exposes item facades through `get`, `iter`, and `entries`. Item updates share their parent's transaction and do not publish a structural change. Registered iterators permit item updates but hold a structural lease until dropped. Structural methods return `CollectionError::Iterating` while leased; an ancestor mutation that could replace the list is rejected before execution.
///
/// Capacity prepares member metadata as well as data slots. Nested collection shapes come from imported data; ordinary insertion and replacement reject an unprepared shape without allocating. Explicit reservation may allocate.
pub struct ModelList<M: Model> {
    identity: u64,
    slots: Vec<Slot>,
    entries: Vec<Entry<M::Owned>>,
    limit: Option<usize>,
    revision: u64,
    binding: Option<Rc<state::ListState<M>>>,
}

impl<M: Model> Default for ModelList<M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M: Model> ModelList<M> {
    pub fn new() -> Self {
        Self {
            identity: next_collection_id(),
            slots: Vec::new(),
            entries: Vec::new(),
            limit: None,
            revision: 0,
            binding: None,
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        let mut list = Self::new();
        list.reserve(capacity).expect("model list capacity");
        list
    }

    pub fn bounded(limit: usize) -> Self {
        let mut list = Self::with_capacity(limit);
        list.limit = Some(limit);
        list
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub(super) fn contains_slot(&self, slot: usize) -> bool {
        self.slots
            .get(slot)
            .is_some_and(|slot| slot.position.is_some())
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.slots.len()
    }

    pub fn limit(&self) -> Option<usize> {
        self.limit
    }

    /// Prepare at least `len() + additional` positions. Removal retains capacity.
    pub fn reserve(&mut self, additional: usize) -> Result<(), CollectionError> {
        let required = self
            .len()
            .checked_add(additional)
            .ok_or(CollectionError::Limit)?;
        if self.limit.is_some_and(|limit| required > limit) {
            return Err(CollectionError::Limit);
        }
        if required <= self.capacity() {
            return Ok(());
        }
        self.entries
            .try_reserve(required - self.entries.len())
            .map_err(|_| CollectionError::Allocation)?;
        self.slots
            .try_reserve(required - self.slots.len())
            .map_err(|_| CollectionError::Allocation)?;
        self.slots.resize_with(required, || Slot {
            generation: 0,
            position: None,
        });
        if let Some(binding) = self.live_binding() {
            binding.prepare(self);
        }
        Ok(())
    }

    pub fn get(&self, index: usize) -> Option<&M::Owned> {
        self.entries.get(index).map(|entry| &entry.value)
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut M::Owned> {
        self.entries.get_mut(index).map(|entry| &mut entry.value)
    }

    pub fn get_by_id(&self, id: ModelItemId) -> Option<&M::Owned> {
        self.position(id).and_then(|index| self.get(index))
    }

    pub fn get_by_id_mut(&mut self, id: ModelItemId) -> Option<&mut M::Owned> {
        self.position(id).and_then(|index| self.get_mut(index))
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &M::Owned> + ExactSizeIterator {
        self.entries.iter().map(|entry| &entry.value)
    }

    pub fn iter_mut(
        &mut self,
    ) -> impl DoubleEndedIterator<Item = &mut M::Owned> + ExactSizeIterator {
        self.entries.iter_mut().map(|entry| &mut entry.value)
    }

    pub fn entries(
        &self,
    ) -> impl DoubleEndedIterator<Item = (ModelItemId, &M::Owned)> + ExactSizeIterator {
        self.entries
            .iter()
            .map(|entry| (self.id(entry.slot), &entry.value))
    }

    pub fn try_push(&mut self, value: M) -> Result<ModelItemId, (CollectionError, M)> {
        self.try_insert(self.len(), value)
    }

    /// Insert without growing storage. Failure returns the untouched input.
    pub fn try_insert(
        &mut self,
        index: usize,
        value: M,
    ) -> Result<ModelItemId, (CollectionError, M)> {
        if index > self.len() {
            return Err((CollectionError::Index, value));
        }
        let binding = self.live_binding();
        let mut value: M::Owned = value.into();
        let mut reason = if self.len() < self.capacity() {
            CollectionError::IdentityExhausted
        } else {
            CollectionError::Capacity
        };
        let slot = self.slots.iter().enumerate().find_map(|(index, slot)| {
            if slot.position.is_some() || slot.generation == u64::MAX {
                return None;
            }
            if let Some(binding) = &binding {
                let node = binding.node(index);
                if !node.accepts(&value.storage().value) {
                    reason = CollectionError::UnpreparedShape;
                    return None;
                }
                if !node.can_activate(value.storage(), binding.owner) {
                    return None;
                }
            }
            Some(index)
        });
        let Some(slot) = slot else {
            return Err((reason, value.into_data()));
        };
        if let Some(binding) = &binding {
            assert!(
                binding
                    .node(slot)
                    .activate(value.storage_mut(), binding.owner)
            );
        }
        self.slots[slot].generation += 1;
        if let Some(binding) = &binding {
            binding.set_generation(slot, self.slots[slot].generation);
        }
        self.entries.insert(index, Entry { slot, value });
        self.reindex(index);
        self.changed();
        Ok(self.id(slot))
    }

    pub fn remove(&mut self, id: ModelItemId) -> Option<M> {
        let index = self.position(id)?;
        if let Some(binding) = self.live_binding() {
            binding.node(id.slot).retire();
        }
        let entry = self.entries.remove(index);
        self.slots[entry.slot].position = None;
        self.reindex(index);
        self.changed();
        Some(entry.value.into_data())
    }

    /// Replace data without changing the member's identity or position.
    pub fn replace(&mut self, id: ModelItemId, value: M) -> Result<M, (CollectionError, M)> {
        let Some(index) = self.position(id) else {
            return Err((CollectionError::Missing, value));
        };
        if let Some(binding) = self.live_binding() {
            if !binding.node(id.slot).accepts(&value) {
                return Err((CollectionError::UnpreparedShape, value));
            }
        }
        Ok(core::mem::replace(&mut self.entries[index].value, value.into()).into_data())
    }

    pub fn clear(&mut self) {
        if self.is_empty() {
            return;
        }
        if let Some(binding) = self.live_binding() {
            for entry in &self.entries {
                binding.node(entry.slot).retire();
            }
        }
        for slot in &mut self.slots {
            slot.position = None;
        }
        self.entries.clear();
        self.changed();
    }

    pub fn retain(&mut self, mut keep: impl FnMut(&M::Owned) -> bool) {
        struct Reindex<'a, M: Model>(&'a mut ModelList<M>, usize);
        impl<M: Model> Drop for Reindex<'_, M> {
            fn drop(&mut self) {
                for slot in &mut self.0.slots {
                    slot.position = None;
                }
                self.0.reindex(0);
                if self.0.len() != self.1 {
                    if let Some(binding) = self.0.live_binding() {
                        for (index, slot) in self.0.slots.iter().enumerate() {
                            if slot.position.is_none() {
                                binding.node(index).retire();
                            }
                        }
                    }
                    self.0.changed();
                }
            }
        }
        let len = self.len();
        let guard = Reindex(self, len);
        guard.0.entries.retain(|entry| keep(&entry.value));
    }

    pub fn swap(&mut self, a: usize, b: usize) -> Result<(), CollectionError> {
        if a >= self.len() || b >= self.len() {
            return Err(CollectionError::Index);
        }
        self.entries.swap(a, b);
        self.reindex(a.min(b));
        if a != b {
            self.changed();
        }
        Ok(())
    }

    pub fn move_to(&mut self, id: ModelItemId, index: usize) -> Result<(), CollectionError> {
        let from = self.position(id).ok_or(CollectionError::Missing)?;
        if index >= self.len() {
            return Err(CollectionError::Index);
        }
        if from < index {
            self.entries[from..=index].rotate_left(1);
        } else {
            self.entries[index..=from].rotate_right(1);
        }
        self.reindex(from.min(index));
        if from != index {
            self.changed();
        }
        Ok(())
    }

    pub fn rotate_left(&mut self, amount: usize) -> Result<(), CollectionError> {
        if self.is_empty() {
            return Ok(());
        }
        if amount > self.len() {
            return Err(CollectionError::Index);
        }
        self.entries.rotate_left(amount);
        self.reindex(0);
        if amount != 0 && amount != self.len() {
            self.changed();
        }
        Ok(())
    }

    fn position(&self, id: ModelItemId) -> Option<usize> {
        if id.collection != self.identity {
            return None;
        }
        let slot = self.slots.get(id.slot)?;
        (slot.generation == id.generation)
            .then_some(slot.position)
            .flatten()
    }

    fn id(&self, slot: usize) -> ModelItemId {
        ModelItemId {
            collection: self.identity,
            slot,
            generation: self.slots[slot].generation,
        }
    }

    fn reindex(&mut self, start: usize) {
        for (index, entry) in self.entries.iter().enumerate().skip(start) {
            self.slots[entry.slot].position = Some(index);
        }
    }

    fn live_binding(&self) -> Option<Rc<state::ListState<M>>> {
        self.binding
            .as_ref()
            .filter(|state| state.active.get())
            .cloned()
    }

    fn changed(&mut self) {
        self.revision = self
            .revision
            .checked_add(1)
            .expect("model list revision exhausted");
        if let Some(binding) = self.live_binding() {
            binding.revision.set(self.revision);
        }
    }
}

impl<M: Model> FromIterator<M> for ModelList<M> {
    fn from_iter<I: IntoIterator<Item = M>>(items: I) -> Self {
        let items = items.into_iter();
        let mut list = Self::with_capacity(items.size_hint().0);
        for value in items {
            if list.len() == list.capacity() {
                list.reserve(list.capacity().max(1))
                    .expect("model list capacity");
            }
            if list.try_push(value).is_err() {
                panic!("model list import capacity exhausted");
            }
        }
        list
    }
}

impl<M: Model> From<Vec<M>> for ModelList<M> {
    fn from(items: Vec<M>) -> Self {
        items.into_iter().collect()
    }
}

impl<M: Model, const N: usize> From<[M; N]> for ModelList<M> {
    fn from(items: [M; N]) -> Self {
        items.into_iter().collect()
    }
}

#[cfg(all(test, feature = "std"))]
mod registered_tests;
#[cfg(all(test, feature = "std"))]
mod tests;
