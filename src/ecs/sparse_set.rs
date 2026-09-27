use alloc::vec::Vec;

use super::entity::Entity;

pub struct SparseSet<T> {
    sparse: Vec<Option<u32>>,
    dense: Vec<Entity>,
    data: Vec<T>,
}

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
            self.data[idx as usize] = value;
            self.dense[idx as usize] = entity;
        } else {
            self.sparse[id] = Some(self.dense.len() as u32);
            self.dense.push(entity);
            self.data.push(value);
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
        self.data.pop()
    }

    pub fn get(&self, entity: Entity) -> Option<&T> {
        let idx = *self.sparse.get(entity.id as usize)?.as_ref()? as usize;
        if self.dense[idx] != entity {
            return None;
        }
        Some(&self.data[idx])
    }

    pub fn get_mut(&mut self, entity: Entity) -> Option<&mut T> {
        let idx = *self.sparse.get(entity.id as usize)?.as_ref()? as usize;
        if self.dense[idx] != entity {
            return None;
        }
        Some(&mut self.data[idx])
    }

    pub fn contains(&self, entity: Entity) -> bool {
        self.get(entity).is_some()
    }

    pub fn len(&self) -> usize {
        self.dense.len()
    }

    pub fn is_empty(&self) -> bool {
        self.dense.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (Entity, &T)> {
        self.dense.iter().copied().zip(self.data.iter())
    }

    pub fn entities(&self) -> &[Entity] {
        &self.dense
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
