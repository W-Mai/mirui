use alloc::collections::BTreeMap;

use crate::ecs::{Entity, World};

pub struct NamedId(pub &'static str);

pub struct IdMap {
    map: BTreeMap<&'static str, Entity>,
}

impl IdMap {
    pub fn new() -> Self {
        Self {
            map: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, id: &'static str, entity: Entity) {
        self.map.insert(id, entity);
    }

    pub fn get(&self, id: &'static str) -> Option<Entity> {
        self.map.get(id).copied()
    }

    pub fn remove(&mut self, id: &'static str) -> Option<Entity> {
        self.map.remove(id)
    }
}

impl Default for IdMap {
    fn default() -> Self {
        Self::new()
    }
}

impl World {
    pub fn find_by_id(&self, id: &'static str) -> Option<Entity> {
        self.resource::<IdMap>().and_then(|m| m.get(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::World;

    fn fresh() -> (World, Entity, Entity) {
        let mut w = World::new();
        let a = w.spawn_empty();
        let b = w.spawn_empty();
        (w, a, b)
    }

    #[test]
    fn insert_then_get() {
        let (_, a, _) = fresh();
        let mut m = IdMap::new();
        m.insert("hero", a);
        assert_eq!(m.get("hero"), Some(a));
    }

    #[test]
    fn missing_id_returns_none() {
        let m = IdMap::new();
        assert_eq!(m.get("ghost"), None);
    }

    #[test]
    fn second_insert_overwrites() {
        let (_, a, b) = fresh();
        let mut m = IdMap::new();
        m.insert("slot", a);
        m.insert("slot", b);
        assert_eq!(m.get("slot"), Some(b));
    }

    #[test]
    fn remove_clears_entry() {
        let (_, a, _) = fresh();
        let mut m = IdMap::new();
        m.insert("hero", a);
        let removed = m.remove("hero");
        assert_eq!(removed, Some(a));
        assert_eq!(m.get("hero"), None);
    }

    #[test]
    fn remove_missing_returns_none() {
        let mut m = IdMap::new();
        assert_eq!(m.remove("ghost"), None);
    }

    #[test]
    fn world_find_by_id_returns_none_without_map() {
        let w = World::new();
        assert_eq!(w.find_by_id("anything"), None);
    }

    #[test]
    fn world_find_by_id_hits_resource() {
        let mut w = World::new();
        w.insert_resource(IdMap::new());
        let e = w.spawn_empty();
        if let Some(map) = w.resource_mut::<IdMap>() {
            map.insert("hero", e);
        }
        assert_eq!(w.find_by_id("hero"), Some(e));
        assert_eq!(w.find_by_id("ghost"), None);
    }
}
