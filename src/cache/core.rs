use core::marker::PhantomData;

#[cfg(not(feature = "sync-cache"))]
use alloc::rc::Rc;
#[cfg(feature = "sync-cache")]
use alloc::sync::Arc as Rc;

use super::algorithm::{Algorithm, Lru};
use super::budget::MaxSize;
use super::handle::{CacheEntry, Handle};
use super::lookup::{HashLookup, Lookup, NodeId};
use super::stats::CacheStats;

type EvictCallback<K, V> = alloc::boxed::Box<dyn FnMut(&K, &V)>;

pub struct Cache<K, V, A = Lru, L = HashLookup<K>>
where
    A: Algorithm,
    L: Lookup<K>,
{
    nodes: Slab<NodeData<K, V>>,
    index: L,
    order: A::State,
    max_size: MaxSize,
    current_size: usize,
    on_evict: Option<EvictCallback<K, V>>,
    stats: CacheStats,
    name: Option<&'static str>,
    _phantom: PhantomData<(K, V, A)>,
}

struct NodeData<K, V> {
    key: K,
    entry: Rc<CacheEntry<V>>,
    size: usize,
}

impl<K, V> Cache<K, V, Lru, HashLookup<K>>
where
    K: core::hash::Hash + Eq,
{
    pub fn builder() -> CacheBuilder<K, V, Lru, HashLookup<K>> {
        CacheBuilder::default()
    }
}

impl<K, V, A, L> Cache<K, V, A, L>
where
    A: Algorithm,
    L: Lookup<K> + Default,
{
    pub fn new(max_size: MaxSize) -> Self {
        Self {
            nodes: Slab::new(),
            index: L::default(),
            order: A::State::default(),
            max_size,
            current_size: 0,
            on_evict: None,
            stats: CacheStats::default(),
            name: None,
            _phantom: PhantomData,
        }
    }
}

impl<K, V, A, L> Cache<K, V, A, L>
where
    A: Algorithm,
    L: Lookup<K>,
{
    pub fn len(&self) -> usize {
        self.index.len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn current_size(&self) -> usize {
        self.current_size
    }
    pub fn max_size(&self) -> &MaxSize {
        &self.max_size
    }
    pub fn stats(&self) -> &CacheStats {
        &self.stats
    }
    pub fn name(&self) -> Option<&'static str> {
        self.name
    }

    pub fn acquire(&mut self, key: &K) -> Option<Handle<V>> {
        let node_id = self.index.get(key)?;
        A::on_access(&mut self.order, node_id);
        self.stats.hit_count += 1;
        Some(Handle {
            inner: self.nodes.get(node_id).entry.clone(),
        })
    }

    pub fn drop(&mut self, key: &K) -> bool {
        self.drop_internal(key).is_some()
    }

    pub fn clear(&mut self) {
        let ids: alloc::vec::Vec<NodeId> = self.nodes.iter_ids().collect();
        for id in ids {
            let node = self.nodes.remove(id);
            self.index.remove(&node.key);
            A::on_remove(&mut self.order, id);
            node.entry.is_invalid.set(true);
            if let Some(cb) = self.on_evict.as_mut() {
                cb(&node.key, &node.entry.payload);
            }
            self.stats.drop_count += 1;
        }
        self.current_size = 0;
    }

    pub fn iter(&self) -> impl Iterator<Item = (&K, Handle<V>)> + '_ {
        self.nodes.iter().map(|nd| {
            (
                &nd.key,
                Handle {
                    inner: nd.entry.clone(),
                },
            )
        })
    }
}

impl<K, V, A, L> Cache<K, V, A, L>
where
    K: Clone,
    A: Algorithm,
    L: Lookup<K>,
{
    /// `K: Clone` because the algorithm only tracks `NodeId`; eviction
    /// has to fish the key out of the slab to drive `drop_internal`.
    pub fn evict_one(&mut self) -> Option<Handle<V>> {
        let victim_id = A::pick_victim(&self.order)?;
        let key = self.nodes.get(victim_id).key.clone();
        self.drop_internal(&key)
    }

    fn evict_one_for_reserve(&mut self) -> bool {
        self.evict_one().is_some()
    }
}

impl<K, V, A, L> Cache<K, V, A, L>
where
    A: Algorithm,
    L: Lookup<K>,
{
    fn drop_internal(&mut self, key: &K) -> Option<Handle<V>> {
        let node_id = self.index.remove(key)?;
        A::on_remove(&mut self.order, node_id);
        let node = self.nodes.remove(node_id);
        self.current_size = self.current_size.saturating_sub(node.size);
        node.entry.is_invalid.set(true);
        if let Some(cb) = self.on_evict.as_mut() {
            cb(&node.key, &node.entry.payload);
        }
        self.stats.drop_count += 1;
        self.stats.evict_count += 1;
        Some(Handle { inner: node.entry })
    }
}

impl<K, V, A, L> Cache<K, V, A, L>
where
    K: Clone,
    A: Algorithm,
    L: Lookup<K>,
{
    pub fn entry(&mut self, key: K) -> Entry<'_, K, V, A, L> {
        if let Some(node_id) = self.index.get(&key) {
            A::on_access(&mut self.order, node_id);
            self.stats.hit_count += 1;
            Entry::Occupied(OccupiedEntry {
                cache: self,
                node_id,
            })
        } else {
            self.stats.miss_count += 1;
            Entry::Vacant(VacantEntry { cache: self, key })
        }
    }

    fn ensure_room_count(&mut self) -> bool {
        match self.max_size {
            MaxSize::Disabled => false,
            MaxSize::Count(limit) => {
                while self.index.len() >= limit && self.evict_one_for_reserve() {}
                self.index.len() < limit
            }
            // bytes-mode enforcement requires a V: HasSize impl block; not
            // wired in v0.19.0, falls through unbounded.
            MaxSize::Bytes(_) => true,
        }
    }

    fn insert_value(&mut self, key: K, value: V, size: usize) -> Handle<V> {
        let entry = Rc::new(CacheEntry::new(value));
        let node_id = self.nodes.insert(NodeData {
            key: key.clone(),
            entry: entry.clone(),
            size,
        });
        self.index.insert(key, node_id);
        A::on_insert(&mut self.order, node_id);
        self.current_size += size;
        self.stats.insert_count += 1;
        Handle { inner: entry }
    }
}

pub enum Entry<'a, K, V, A: Algorithm, L: Lookup<K>>
where
    K: Clone,
{
    Occupied(OccupiedEntry<'a, K, V, A, L>),
    Vacant(VacantEntry<'a, K, V, A, L>),
}

pub struct OccupiedEntry<'a, K, V, A: Algorithm, L: Lookup<K>>
where
    K: Clone,
{
    cache: &'a mut Cache<K, V, A, L>,
    node_id: NodeId,
}

pub struct VacantEntry<'a, K, V, A: Algorithm, L: Lookup<K>>
where
    K: Clone,
{
    cache: &'a mut Cache<K, V, A, L>,
    key: K,
}

impl<'a, K, V, A: Algorithm, L: Lookup<K>> Entry<'a, K, V, A, L>
where
    K: Clone,
{
    pub fn key(&self) -> &K {
        match self {
            Entry::Occupied(o) => &o.cache.nodes.get(o.node_id).key,
            Entry::Vacant(v) => &v.key,
        }
    }

    pub fn or_insert_with<F: FnOnce() -> V>(self, factory: F) -> Handle<V> {
        match self {
            Entry::Occupied(o) => o.into_handle(),
            Entry::Vacant(v) => v.insert(factory()),
        }
    }

    pub fn or_try_insert_with<F, E>(self, factory: F) -> Result<Handle<V>, E>
    where
        F: FnOnce() -> Result<V, E>,
    {
        match self {
            Entry::Occupied(o) => Ok(o.into_handle()),
            Entry::Vacant(v) => Ok(v.insert(factory()?)),
        }
    }
}

impl<'a, K, V, A: Algorithm, L: Lookup<K>> OccupiedEntry<'a, K, V, A, L>
where
    K: Clone,
{
    pub fn handle(&self) -> Handle<V> {
        Handle {
            inner: self.cache.nodes.get(self.node_id).entry.clone(),
        }
    }

    pub fn into_handle(self) -> Handle<V> {
        self.handle()
    }

    pub fn key(&self) -> &K {
        &self.cache.nodes.get(self.node_id).key
    }
}

impl<'a, K, V, A: Algorithm, L: Lookup<K>> VacantEntry<'a, K, V, A, L>
where
    K: Clone,
{
    pub fn key(&self) -> &K {
        &self.key
    }

    pub fn insert(self, value: V) -> Handle<V> {
        self.cache.ensure_room_count();
        self.cache.insert_value(self.key, value, 1)
    }
}

pub struct CacheBuilder<K, V, A: Algorithm, L: Lookup<K>> {
    max_size: MaxSize,
    on_evict: Option<EvictCallback<K, V>>,
    name: Option<&'static str>,
    _phantom: PhantomData<(K, V, A, L)>,
}

impl<K, V, A: Algorithm, L: Lookup<K>> Default for CacheBuilder<K, V, A, L> {
    fn default() -> Self {
        Self {
            max_size: MaxSize::default(),
            on_evict: None,
            name: None,
            _phantom: PhantomData,
        }
    }
}

impl<K, V, A: Algorithm, L: Lookup<K> + Default> CacheBuilder<K, V, A, L> {
    pub fn max_size(mut self, m: MaxSize) -> Self {
        self.max_size = m;
        self
    }

    pub fn on_evict<F: FnMut(&K, &V) + 'static>(mut self, f: F) -> Self {
        self.on_evict = Some(alloc::boxed::Box::new(f));
        self
    }

    pub fn name(mut self, n: &'static str) -> Self {
        self.name = Some(n);
        self
    }

    pub fn build(self) -> Cache<K, V, A, L> {
        Cache {
            nodes: Slab::new(),
            index: L::default(),
            order: A::State::default(),
            max_size: self.max_size,
            current_size: 0,
            on_evict: self.on_evict,
            stats: CacheStats::default(),
            name: self.name,
            _phantom: PhantomData,
        }
    }
}

// Ids are stable across other inserts/removes and dense (freed slots
// are reused before extending), which lets the algorithm's intrusive
// list index by id without reallocating its slot table.
struct Slab<T> {
    entries: alloc::vec::Vec<SlabSlot<T>>,
    next_free: Option<usize>,
    len: usize,
}

enum SlabSlot<T> {
    Occupied(T),
    Vacant { next_free: Option<usize> },
}

impl<T> Slab<T> {
    fn new() -> Self {
        Self {
            entries: alloc::vec::Vec::new(),
            next_free: None,
            len: 0,
        }
    }

    fn insert(&mut self, value: T) -> usize {
        self.len += 1;
        if let Some(idx) = self.next_free {
            let SlabSlot::Vacant { next_free } = &self.entries[idx] else {
                unreachable!("slab next_free pointed at occupied slot")
            };
            self.next_free = *next_free;
            self.entries[idx] = SlabSlot::Occupied(value);
            idx
        } else {
            let idx = self.entries.len();
            self.entries.push(SlabSlot::Occupied(value));
            idx
        }
    }

    fn remove(&mut self, id: usize) -> T {
        let slot = core::mem::replace(
            &mut self.entries[id],
            SlabSlot::Vacant {
                next_free: self.next_free,
            },
        );
        self.next_free = Some(id);
        self.len -= 1;
        match slot {
            SlabSlot::Occupied(v) => v,
            SlabSlot::Vacant { .. } => panic!("slab removing already-vacant slot {id}"),
        }
    }

    fn get(&self, id: usize) -> &T {
        match &self.entries[id] {
            SlabSlot::Occupied(v) => v,
            SlabSlot::Vacant { .. } => panic!("slab access on vacant slot {id}"),
        }
    }

    fn iter(&self) -> impl Iterator<Item = &T> + '_ {
        self.entries.iter().filter_map(|s| match s {
            SlabSlot::Occupied(v) => Some(v),
            SlabSlot::Vacant { .. } => None,
        })
    }

    fn iter_ids(&self) -> impl Iterator<Item = usize> + '_ {
        self.entries
            .iter()
            .enumerate()
            .filter_map(|(i, s)| match s {
                SlabSlot::Occupied(_) => Some(i),
                SlabSlot::Vacant { .. } => None,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_mode_evicts_lru_on_capacity_overflow() {
        let mut cache: Cache<u32, alloc::string::String, Lru, HashLookup<u32>> =
            Cache::builder().max_size(MaxSize::Count(2)).build();

        let h1 = cache.entry(1).or_insert_with(|| "a".into());
        let _h2 = cache.entry(2).or_insert_with(|| "b".into());
        // 2 entries, at capacity. Touch 1 so 2 becomes LRU.
        let _h1_again = cache.acquire(&1).unwrap();
        let _h3 = cache.entry(3).or_insert_with(|| "c".into());
        // 2 should be evicted, 1 and 3 remain.
        assert!(cache.acquire(&1).is_some());
        assert!(cache.acquire(&3).is_some());
        assert!(cache.acquire(&2).is_none());
        // Old handle to 1 still valid (Rc shared payload).
        assert_eq!(&*h1, "a");
    }

    #[test]
    fn entry_or_insert_with_returns_existing_handle_on_hit() {
        let mut cache: Cache<u32, u32, Lru, HashLookup<u32>> =
            Cache::builder().max_size(MaxSize::Count(8)).build();
        let h1 = cache.entry(1).or_insert_with(|| 100);
        let h2 = cache.entry(1).or_insert_with(|| {
            panic!("factory must not run on hit");
        });
        assert_eq!(*h1, 100);
        assert_eq!(*h2, 100);
        assert_eq!(cache.stats().hit_count, 1);
        assert_eq!(cache.stats().miss_count, 1);
    }

    #[test]
    fn drop_marks_invalid_and_keeps_outside_handles_alive() {
        let mut cache: Cache<u32, alloc::string::String, Lru, HashLookup<u32>> =
            Cache::builder().max_size(MaxSize::Count(4)).build();
        let h = cache.entry(1).or_insert_with(|| "x".into());
        assert!(!h.is_invalid());
        cache.drop(&1);
        assert!(h.is_invalid());
        assert_eq!(&*h, "x");
        assert!(cache.acquire(&1).is_none());
    }

    #[test]
    fn on_evict_callback_fires_with_key_and_value() {
        use core::cell::RefCell;
        let log: alloc::rc::Rc<RefCell<alloc::vec::Vec<(u32, alloc::string::String)>>> =
            alloc::rc::Rc::new(RefCell::new(alloc::vec::Vec::new()));
        let log_for_cb = log.clone();
        let mut cache: Cache<u32, alloc::string::String, Lru, HashLookup<u32>> = Cache::builder()
            .max_size(MaxSize::Count(1))
            .on_evict(move |k: &u32, v: &alloc::string::String| {
                log_for_cb.borrow_mut().push((*k, v.clone()));
            })
            .build();
        cache.entry(1).or_insert_with(|| "a".into());
        cache.entry(2).or_insert_with(|| "b".into());
        let entries = log.borrow();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0], (1, "a".into()));
    }
}
