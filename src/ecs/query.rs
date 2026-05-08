use alloc::vec::Vec;
use core::any::TypeId;
use core::marker::PhantomData;

use super::entity::Entity;
use super::world::World;

pub struct QueryBuilder<'w, T: 'static> {
    world: &'w World,
    and_filters: Vec<TypeId>,
    without_filters: Vec<TypeId>,
    _marker: PhantomData<T>,
}

impl<'w, T: 'static> QueryBuilder<'w, T> {
    pub(crate) fn new(world: &'w World) -> Self {
        Self {
            world,
            and_filters: Vec::new(),
            without_filters: Vec::new(),
            _marker: PhantomData,
        }
    }

    pub fn and<U: 'static>(mut self) -> Self {
        self.and_filters.push(TypeId::of::<U>());
        self
    }

    pub fn without<U: 'static>(mut self) -> Self {
        self.without_filters.push(TypeId::of::<U>());
        self
    }

    fn matches(&self, entity: Entity) -> bool {
        for &tid in &self.and_filters {
            if !self.world.has_type(entity, tid) {
                return false;
            }
        }
        for &tid in &self.without_filters {
            if self.world.has_type(entity, tid) {
                return false;
            }
        }
        true
    }

    /// Iterate over (Entity, &T) for all matching entities
    pub fn iter(&self) -> impl Iterator<Item = (Entity, &T)> + '_ {
        self.world
            .storage::<T>()
            .into_iter()
            .flat_map(|s| s.iter())
            .filter(|(e, _)| self.matches(*e))
    }

    /// Collect matching entity IDs into a user-provided buffer (zero alloc after first frame)
    pub fn collect_into(self, buf: &mut Vec<Entity>) {
        buf.clear();
        if let Some(storage) = self.world.storage::<T>() {
            for &entity in storage.entities() {
                if self.matches(entity) {
                    buf.push(entity);
                }
            }
        }
    }

    /// Collect matching entity IDs into a new Vec
    pub fn collect(self) -> Vec<Entity> {
        let mut buf = Vec::new();
        self.collect_into(&mut buf);
        buf
    }
}
