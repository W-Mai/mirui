use alloc::collections::BTreeMap;

use crate::ecs::Entity;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::World;

    fn fresh() -> (World, Entity, Entity) {
        let mut w = World::new();
        let a = w.spawn();
        let b = w.spawn();
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
}
