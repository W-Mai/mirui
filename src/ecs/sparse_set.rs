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
