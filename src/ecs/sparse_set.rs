use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::marker::PhantomData;

use super::entity::Entity;

/// Pointer-only snapshot of one sparse set during a stable query World window.
///
/// The snapshot never changes storage structure. Its caller must keep the originating World borrowed and prevent insert/remove/despawn until every snapshot and reference produced from it has been dropped. Shared World windows permit only shared value reads; exclusive windows validate every read/write access before capturing parts.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct SparseSetParts<'world, T> {
    sparse: *const Option<u32>,
    sparse_len: usize,
    dense: *const Entity,
    dense_len: usize,
    data: *const UnsafeCell<T>,
    _world: PhantomData<&'world mut T>,
}

impl<T> Copy for SparseSetParts<'_, T> {}

impl<T> Clone for SparseSetParts<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

#[cfg_attr(not(test), allow(dead_code))]
impl<'world, T> SparseSetParts<'world, T> {
    pub(crate) const fn len(self) -> usize {
        self.dense_len
    }

    /// # Safety
    ///
    /// `index` must be smaller than [`Self::len`], and the originating sparse
    /// set must retain its structure for `'world`.
    pub(crate) unsafe fn entity_unchecked(self, index: usize) -> Entity {
        // SAFETY: guaranteed by the caller; dense stores initialized Entities.
        unsafe { self.dense.add(index).read() }
    }

    fn dense_index(self, entity: Entity) -> Option<usize> {
        let sparse_index = entity.id as usize;
        if sparse_index >= self.sparse_len {
            return None;
        }
        // SAFETY: sparse_index was checked against the captured slice length.
        let dense_index = unsafe { self.sparse.add(sparse_index).read()? as usize };
        if dense_index >= self.dense_len {
            return None;
        }
        // SAFETY: dense_index was checked against the captured slice length.
        (unsafe { self.dense.add(dense_index).read() } == entity).then_some(dense_index)
    }

    pub(crate) fn contains(self, entity: Entity) -> bool {
        self.dense_index(entity).is_some()
    }

    /// # Safety
    ///
    /// No mutable reference to this component slot may be alive. The
    /// originating sparse set must retain its structure for `'world`.
    pub(crate) unsafe fn get(self, entity: Entity) -> Option<&'world T> {
        let index = self.dense_index(entity)?;
        // SAFETY: dense and data use the same index, and the caller guarantees
        // shared access is compatible with every other live query reference.
        Some(unsafe { &*UnsafeCell::raw_get(self.data.add(index)) })
    }

    /// # Safety
    ///
    /// This slot must not have any other live reference. The originating
    /// sparse set must retain its structure for `'world`.
    pub(crate) unsafe fn get_mut(self, entity: Entity) -> Option<&'world mut T> {
        let index = self.dense_index(entity)?;
        // SAFETY: dense and data use the same index, and the caller guarantees
        // this entity/component pair is yielded at most once with write access.
        Some(unsafe { &mut *UnsafeCell::raw_get(self.data.add(index)) })
    }
}

pub struct SparseSet<T> {
    sparse: Vec<Option<u32>>,
    dense: Vec<Entity>,
    // Query snapshots are intentionally created through shared SparseSet
    // references: the driver, filters, and row data may all target the same
    // storage. UnsafeCell keeps those snapshots from manufacturing overlapping
    // `&mut SparseSet<T>` borrows while access validation still guarantees that
    // each value slot has either shared readers or one writer.
    data: Vec<UnsafeCell<T>>,
}

// UnsafeCell only protects the crate-private typed-query snapshot path. Public
// SparseSet mutation still requires `&mut self`, so preserve the auto-traits
// the previous `Vec<T>` representation provided.
unsafe impl<T: Send> Send for SparseSet<T> {}
unsafe impl<T: Sync> Sync for SparseSet<T> {}

impl<T> Default for SparseSet<T> {
    fn default() -> Self {
        Self {
            sparse: Vec::new(),
            dense: Vec::new(),
            data: Vec::new(),
        }
    }
}

impl<T> SparseSet<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn reserve_entities(&mut self, max_entity_id: u32, additional: usize) {
        let sparse_len = max_entity_id as usize + 1;
        if self.sparse.len() < sparse_len {
            self.sparse.resize(sparse_len, None);
        }
        self.dense.reserve(additional);
        self.data.reserve(additional);
    }

    pub fn insert(&mut self, entity: Entity, value: T) {
        let id = entity.id as usize;
        if id >= self.sparse.len() {
            self.sparse.resize(id + 1, None);
        }
        if let Some(idx) = self.sparse[id] {
            *self.data[idx as usize].get_mut() = value;
            self.dense[idx as usize] = entity;
        } else {
            self.sparse[id] = Some(self.dense.len() as u32);
            self.dense.push(entity);
            self.data.push(UnsafeCell::new(value));
        }
    }

    pub fn remove(&mut self, entity: Entity) -> Option<T> {
        let id = entity.id as usize;
        let idx = *self.sparse.get(id)?.as_ref()? as usize;
        if self.dense[idx] != entity {
            return None;
        }
        self.sparse[id] = None;
        let last = self.dense.len() - 1;
        if idx != last {
            let moved_entity = self.dense[last];
            self.sparse[moved_entity.id as usize] = Some(idx as u32);
            self.dense.swap(idx, last);
            self.data.swap(idx, last);
        }
        self.dense.pop();
        self.data.pop().map(UnsafeCell::into_inner)
    }

    pub fn get(&self, entity: Entity) -> Option<&T> {
        let idx = *self.sparse.get(entity.id as usize)?.as_ref()? as usize;
        if self.dense[idx] != entity {
            return None;
        }
        // SAFETY: safe mutation requires `&mut self`; crate-private query
        // snapshots only write within a validated exclusive World window.
        // Shared query windows produce compatible shared value references.
        Some(unsafe { &*self.data[idx].get() })
    }

    pub fn get_mut(&mut self, entity: Entity) -> Option<&mut T> {
        let idx = *self.sparse.get(entity.id as usize)?.as_ref()? as usize;
        if self.dense[idx] != entity {
            return None;
        }
        Some(self.data[idx].get_mut())
    }

    pub fn contains(&self, entity: Entity) -> bool {
        self.get(entity).is_some()
    }

    pub fn len(&self) -> usize {
        self.dense.len()
    }

    #[cfg(test)]
    pub(crate) fn reserved_entity_capacity(&self) -> (usize, usize) {
        (self.sparse.len(), self.dense.capacity())
    }

    pub fn is_empty(&self) -> bool {
        self.dense.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (Entity, &T)> {
        self.dense.iter().copied().zip(
            self.data
                .iter()
                // SAFETY: see `get`; safe SparseSet mutation needs `&mut self`.
                .map(|value| unsafe { &*value.get() }),
        )
    }

    pub fn entities(&self) -> &[Entity] {
        &self.dense
    }

    #[cfg_attr(not(test), allow(dead_code))]
    /// # Safety
    ///
    /// The caller must keep the storage structure fixed and validate every
    /// value access represented by all live snapshots.
    pub(crate) unsafe fn parts(&self) -> SparseSetParts<'_, T> {
        SparseSetParts {
            sparse: self.sparse.as_ptr(),
            sparse_len: self.sparse.len(),
            dense: self.dense.as_ptr(),
            dense_len: self.dense.len(),
            data: self.data.as_ptr(),
            _world: PhantomData,
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::rc::Rc;
    use core::cell::Cell;

    use super::*;

    struct DropProbe(Rc<Cell<usize>>);

    impl Drop for DropProbe {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    #[test]
    fn reserved_entities_do_not_create_components_or_grow_on_insert() {
        let mut storage = SparseSet::<u16>::new();
        storage.reserve_entities(7, 2);
        let sparse_capacity = storage.sparse.capacity();
        let dense_capacity = storage.dense.capacity();
        let data_capacity = storage.data.capacity();
        assert_eq!(storage.len(), 0);

        let first = Entity {
            id: 3,
            generation: 0,
        };
        let second = Entity {
            id: 7,
            generation: 0,
        };
        storage.insert(first, 10);
        storage.insert(second, 20);
        assert_eq!(storage.get(first), Some(&10));
        assert_eq!(storage.get(second), Some(&20));
        assert_eq!(storage.sparse.capacity(), sparse_capacity);
        assert_eq!(storage.dense.capacity(), dense_capacity);
        assert_eq!(storage.data.capacity(), data_capacity);
    }

    #[test]
    fn value_cells_preserve_replace_remove_and_storage_drop() {
        let drops = Rc::new(Cell::new(0));
        let first = Entity {
            id: 0,
            generation: 0,
        };
        let second = Entity {
            id: 1,
            generation: 0,
        };
        let removed = {
            let mut storage = SparseSet::new();
            storage.insert(first, DropProbe(Rc::clone(&drops)));
            storage.insert(second, DropProbe(Rc::clone(&drops)));
            storage.insert(first, DropProbe(Rc::clone(&drops)));
            assert_eq!(drops.get(), 1, "replacement drops the old value");
            assert_eq!(storage.iter().count(), 2);
            storage.remove(second).expect("second value")
        };

        assert_eq!(drops.get(), 2, "storage drop retains only its live value");
        drop(removed);
        assert_eq!(drops.get(), 3, "removed value keeps ordinary ownership");
    }
}
