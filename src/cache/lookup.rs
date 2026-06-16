use core::hash::Hash;
use rustc_hash::FxBuildHasher;

pub type NodeId = usize;

pub trait Lookup<K> {
    fn get(&self, key: &K) -> Option<NodeId>;
    fn insert(&mut self, key: K, node: NodeId);
    fn remove(&mut self, key: &K) -> Option<NodeId>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn clear(&mut self);
}

pub struct OrdLookup<K: Ord> {
    map: alloc::collections::BTreeMap<K, NodeId>,
}

impl<K: Ord> Default for OrdLookup<K> {
    fn default() -> Self {
        Self {
            map: alloc::collections::BTreeMap::new(),
        }
    }
}

impl<K: Ord> Lookup<K> for OrdLookup<K> {
    fn get(&self, key: &K) -> Option<NodeId> {
        self.map.get(key).copied()
    }
    fn insert(&mut self, key: K, node: NodeId) {
        self.map.insert(key, node);
    }
    fn remove(&mut self, key: &K) -> Option<NodeId> {
        self.map.remove(key)
    }
    fn len(&self) -> usize {
        self.map.len()
    }
    fn clear(&mut self) {
        self.map.clear();
    }
}

pub struct HashLookup<K: Hash + Eq> {
    map: hashbrown::HashMap<K, NodeId, FxBuildHasher>,
}

impl<K: Hash + Eq> Default for HashLookup<K> {
    fn default() -> Self {
        Self {
            map: hashbrown::HashMap::default(),
        }
    }
}

impl<K: Hash + Eq> Lookup<K> for HashLookup<K> {
    fn get(&self, key: &K) -> Option<NodeId> {
        self.map.get(key).copied()
    }
    fn insert(&mut self, key: K, node: NodeId) {
        self.map.insert(key, node);
    }
    fn remove(&mut self, key: &K) -> Option<NodeId> {
        self.map.remove(key)
    }
    fn len(&self) -> usize {
        self.map.len()
    }
    fn clear(&mut self) {
        self.map.clear();
    }
}

pub struct LinearLookup<K: PartialEq> {
    entries: alloc::vec::Vec<(K, NodeId)>,
}

impl<K: PartialEq> Default for LinearLookup<K> {
    fn default() -> Self {
        Self {
            entries: alloc::vec::Vec::new(),
        }
    }
}

impl<K: PartialEq> Lookup<K> for LinearLookup<K> {
    fn get(&self, key: &K) -> Option<NodeId> {
        self.entries.iter().find(|(k, _)| k == key).map(|(_, n)| *n)
    }
    fn insert(&mut self, key: K, node: NodeId) {
        if let Some(slot) = self.entries.iter_mut().find(|(k, _)| *k == key) {
            slot.1 = node;
        } else {
            self.entries.push((key, node));
        }
    }
    fn remove(&mut self, key: &K) -> Option<NodeId> {
        let idx = self.entries.iter().position(|(k, _)| k == key)?;
        Some(self.entries.swap_remove(idx).1)
    }
    fn len(&self) -> usize {
        self.entries.len()
    }
    fn clear(&mut self) {
        self.entries.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check<L: Lookup<u32> + Default>() {
        let mut l = L::default();
        assert_eq!(l.get(&1), None);
        l.insert(1, 100);
        l.insert(2, 200);
        assert_eq!(l.get(&1), Some(100));
        assert_eq!(l.get(&2), Some(200));
        assert_eq!(l.len(), 2);
        assert_eq!(l.remove(&1), Some(100));
        assert_eq!(l.get(&1), None);
        assert_eq!(l.len(), 1);
    }

    #[test]
    fn ord_lookup_basic() {
        check::<OrdLookup<u32>>();
    }
    #[test]
    fn hash_lookup_basic() {
        check::<HashLookup<u32>>();
    }
    #[test]
    fn linear_lookup_basic() {
        check::<LinearLookup<u32>>();
    }
}
