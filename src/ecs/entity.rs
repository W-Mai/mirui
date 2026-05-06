use alloc::vec::Vec;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Entity {
    pub id: u32,
    pub generation: u32,
}

#[derive(Default)]
pub struct EntityAllocator {
    generations: Vec<u32>,
    free_ids: Vec<u32>,
    next_id: u32,
}

impl EntityAllocator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn allocate(&mut self) -> Entity {
        if let Some(id) = self.free_ids.pop() {
            Entity {
                id,
                generation: self.generations[id as usize],
            }
        } else {
            let id = self.next_id;
            self.next_id += 1;
            self.generations.push(0);
            Entity { id, generation: 0 }
        }
    }

    pub fn deallocate(&mut self, entity: Entity) -> bool {
        if !self.is_alive(entity) {
            return false;
        }
        self.generations[entity.id as usize] += 1;
        self.free_ids.push(entity.id);
        true
    }

    pub fn is_alive(&self, entity: Entity) -> bool {
        (entity.id as usize) < self.generations.len()
            && self.generations[entity.id as usize] == entity.generation
    }
}
