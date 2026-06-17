use core::borrow::Borrow;
use core::hash::Hash;
use rustc_hash::FxBuildHasher;

pub type NodeId = usize;

pub trait Lookup<K> {
    fn get<Q>(&self, key: &Q) -> Option<NodeId>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized;

    fn insert(&mut self, key: K, node: NodeId);

    fn remove<Q>(&mut self, key: &Q) -> Option<NodeId>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized;

    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn clear(&mut self);
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
    fn get<Q>(&self, key: &Q) -> Option<NodeId>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.map.get(key).copied()
    }
    fn insert(&mut self, key: K, node: NodeId) {
        self.map.insert(key, node);
    }
    fn remove<Q>(&mut self, key: &Q) -> Option<NodeId>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
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
    fn get<Q>(&self, key: &Q) -> Option<NodeId>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.entries
            .iter()
            .find(|(k, _)| k.borrow() == key)
            .map(|(_, n)| *n)
    }
    fn insert(&mut self, key: K, node: NodeId) {
        if let Some(slot) = self.entries.iter_mut().find(|(k, _)| *k == key) {
            slot.1 = node;
        } else {
            self.entries.push((key, node));
        }
    }
    fn remove<Q>(&mut self, key: &Q) -> Option<NodeId>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let idx = self.entries.iter().position(|(k, _)| k.borrow() == key)?;
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
    fn hash_lookup_basic() {
        check::<HashLookup<u32>>();
    }
    #[test]
    fn linear_lookup_basic() {
        check::<LinearLookup<u32>>();
    }

    #[test]
    fn hash_lookup_borrowed_str_query() {
        let mut l = HashLookup::<alloc::string::String>::default();
        l.insert("foo".into(), 7);
        assert_eq!(l.get("foo"), Some(7));
        assert_eq!(l.remove("foo"), Some(7));
        assert_eq!(l.get("foo"), None);
    }

    #[test]
    fn linear_lookup_borrowed_str_query() {
        let mut l = LinearLookup::<alloc::string::String>::default();
        l.insert("foo".into(), 7);
        assert_eq!(l.get("foo"), Some(7));
        assert_eq!(l.remove("foo"), Some(7));
        assert_eq!(l.get("foo"), None);
    }
}
