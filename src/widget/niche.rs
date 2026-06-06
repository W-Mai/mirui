use alloc::collections::BTreeMap;

use crate::ecs::Entity;

pub struct NicheMap {
    map: BTreeMap<&'static str, Entity>,
}

impl NicheMap {
    pub fn new() -> Self {
        Self {
            map: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, name: &'static str, entity: Entity) {
        self.map.insert(name, entity);
    }

    pub fn get(&self, name: &str) -> Option<Entity> {
        self.map.get(name).copied()
    }

    pub fn keys(&self) -> impl Iterator<Item = &&'static str> {
        self.map.keys()
    }

    pub fn from<const N: usize>(entries: [(&'static str, Entity); N]) -> Self {
        Self {
            map: BTreeMap::from(entries),
        }
    }
}

impl Default for NicheMap {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::World;

    #[test]
    fn insert_and_get() {
        let mut w = World::new();
        let e = w.spawn();
        let mut m = NicheMap::new();
        m.insert("body", e);
        assert_eq!(m.get("body"), Some(e));
        assert_eq!(m.get("missing"), None);
    }

    #[test]
    fn from_array() {
        let mut w = World::new();
        let a = w.spawn();
        let b = w.spawn();
        let m = NicheMap::from([("header", a), ("body", b)]);
        assert_eq!(m.get("header"), Some(a));
        assert_eq!(m.get("body"), Some(b));
    }

    #[test]
    fn keys_lists_registered_names() {
        let mut w = World::new();
        let a = w.spawn();
        let b = w.spawn();
        let m = NicheMap::from([("a", a), ("b", b)]);
        let names: alloc::vec::Vec<_> = m.keys().copied().collect();
        assert_eq!(names, alloc::vec!["a", "b"]);
    }
}
