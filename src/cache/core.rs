use core::marker::PhantomData;

#[cfg(not(feature = "sync-cache"))]
use alloc::rc::Rc;
#[cfg(feature = "sync-cache")]
use alloc::sync::Arc as Rc;

use super::algorithm::{Algorithm, Lru};
use super::budget::{HasSize, MaxSize};
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
        let Some(node_id) = self.index.get(key) else {
            self.stats.miss_count += 1;
            return None;
        };
        A::on_access(&mut self.order, node_id);
        self.stats.hit_count += 1;
        Some(Handle {
            inner: self.nodes.get(node_id).entry.clone(),
        })
    }

    pub fn drop(&mut self, key: &K) -> bool {
        self.remove_internal(key, RemoveReason::Drop).is_some()
    }

    pub fn clear(&mut self) {
        let ids: alloc::vec::Vec<NodeId> = self.nodes.iter_ids().collect();
        for id in ids {
            let node = self.nodes.remove(id);
            self.index.remove(&node.key);
            A::on_remove(&mut self.order, id);
            node.entry.invalid_set(true);
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
    /// has to fish the key out of the slab to drive `remove_internal`.
    pub fn evict_one(&mut self) -> Option<Handle<V>> {
        let victim_id = A::pick_victim(&self.order)?;
        let key = self.nodes.get(victim_id).key.clone();
        self.remove_internal(&key, RemoveReason::Evict)
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
    fn remove_internal(&mut self, key: &K, reason: RemoveReason) -> Option<Handle<V>> {
        let node_id = self.index.remove(key)?;
        A::on_remove(&mut self.order, node_id);
        let node = self.nodes.remove(node_id);
        self.current_size = self.current_size.saturating_sub(node.size);
        node.entry.invalid_set(true);
        // on_evict fires only on algorithm-driven removal; user-initiated
        // drop / clear are silent because the caller already knows.
        match reason {
            RemoveReason::Drop => self.stats.drop_count += 1,
            RemoveReason::Evict => {
                self.stats.evict_count += 1;
                if let Some(cb) = self.on_evict.as_mut() {
                    cb(&node.key, &node.entry.payload);
                }
            }
        }
        Some(Handle { inner: node.entry })
    }
}

#[derive(Clone, Copy)]
enum RemoveReason {
    Drop,
    Evict,
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

    /// Make space for an incoming entry of `needed_size` bytes. Returns
    /// false if the cache cannot or will not accept it (Disabled,
    /// Count(0), or Bytes(0); or Bytes(limit) with `needed_size > limit`
    /// — a single entry that exceeds the whole budget is rejected
    /// rather than evicting everything and still failing).
    fn ensure_room(&mut self, needed_size: usize) -> bool {
        match self.max_size {
            MaxSize::Disabled => false,
            MaxSize::Count(0) => false,
            MaxSize::Count(limit) => {
                while self.index.len() >= limit && self.evict_one_for_reserve() {}
                self.index.len() < limit
            }
            MaxSize::Bytes(0) => false,
            MaxSize::Bytes(limit) => {
                if needed_size > limit {
                    return false;
                }
                while self.current_size + needed_size > limit && self.evict_one_for_reserve() {}
                self.current_size + needed_size <= limit
            }
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
}

impl<'a, K, V, A: Algorithm, L: Lookup<K>> Entry<'a, K, V, A, L>
where
    K: Clone,
    V: HasSize,
{
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
}

impl<'a, K, V, A: Algorithm, L: Lookup<K>> VacantEntry<'a, K, V, A, L>
where
    K: Clone,
    V: HasSize,
{
    /// On Disabled / Count(0) / Bytes(0), or when the value alone exceeds
    /// a Bytes budget, the value is wrapped as an already-invalid detached
    /// Handle — the chain still types, but `h.is_invalid()` is the signal
    /// the value never made it into the cache.
    pub fn insert(self, value: V) -> Handle<V> {
        let size = value.cache_size().max(1);
        if self.cache.ensure_room(size) {
            self.cache.insert_value(self.key, value, size)
        } else {
            let entry = CacheEntry::new(value);
            entry.invalid_set(true);
            Handle::from_rc(Rc::new(entry))
        }
    }
}

pub struct CacheBuilder<K, V, A: Algorithm, L: Lookup<K>> {
    max_size: Option<MaxSize>,
    on_evict: Option<EvictCallback<K, V>>,
    name: Option<&'static str>,
    _phantom: PhantomData<(K, V, A, L)>,
}

impl<K, V, A: Algorithm, L: Lookup<K>> Default for CacheBuilder<K, V, A, L> {
    fn default() -> Self {
        Self {
            max_size: None,
            on_evict: None,
            name: None,
            _phantom: PhantomData,
        }
    }
}

impl<K, V, A: Algorithm, L: Lookup<K> + Default> CacheBuilder<K, V, A, L> {
    pub fn max_size(mut self, m: MaxSize) -> Self {
        self.max_size = Some(m);
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

    /// Panics if `max_size` was never set.
    pub fn build(self) -> Cache<K, V, A, L> {
        let max_size = self
            .max_size
            .expect("CacheBuilder::max_size must be configured before build");
        Cache {
            nodes: Slab::new(),
            index: L::default(),
            order: A::State::default(),
            max_size,
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
        // Check before mutate: a double-remove panics with len/next_free intact.
        if !matches!(self.entries[id], SlabSlot::Occupied(_)) {
            panic!("slab removing already-vacant slot {id}");
        }
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
            SlabSlot::Vacant { .. } => unreachable!("checked occupancy above"),
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
        let _h1_again = cache.acquire(&1).unwrap();
        let _h3 = cache.entry(3).or_insert_with(|| "c".into());
        assert!(cache.acquire(&1).is_some());
        assert!(cache.acquire(&3).is_some());
        assert!(cache.acquire(&2).is_none());
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
    fn on_evict_callback_fires_only_on_algorithmic_eviction() {
        use core::cell::RefCell;
        let log: alloc::rc::Rc<RefCell<alloc::vec::Vec<(u32, alloc::string::String)>>> =
            alloc::rc::Rc::new(RefCell::new(alloc::vec::Vec::new()));
        let log_for_cb = log.clone();
        let mut cache: Cache<u32, alloc::string::String, Lru, HashLookup<u32>> = Cache::builder()
            .max_size(MaxSize::Count(2))
            .on_evict(move |k: &u32, v: &alloc::string::String| {
                log_for_cb.borrow_mut().push((*k, v.clone()));
            })
            .build();
        cache.entry(1).or_insert_with(|| "a".into());
        cache.entry(2).or_insert_with(|| "b".into());
        cache.drop(&1);
        assert!(log.borrow().is_empty(), "drop must not fire on_evict");
        cache.clear();
        assert!(log.borrow().is_empty(), "clear must not fire on_evict");
        cache.entry(10).or_insert_with(|| "x".into());
        cache.entry(11).or_insert_with(|| "y".into());
        cache.entry(12).or_insert_with(|| "z".into()); // evicts 10
        let entries = log.borrow();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0], (10, "x".into()));
    }

    #[test]
    fn drop_and_evict_count_separately() {
        let mut cache: Cache<u32, u32, Lru, HashLookup<u32>> =
            Cache::builder().max_size(MaxSize::Count(2)).build();
        cache.entry(1).or_insert_with(|| 1);
        cache.entry(2).or_insert_with(|| 2);
        cache.drop(&1);
        assert_eq!(cache.stats().drop_count, 1);
        assert_eq!(cache.stats().evict_count, 0);

        cache.entry(3).or_insert_with(|| 3);
        cache.entry(4).or_insert_with(|| 4); // evicts 2
        assert_eq!(cache.stats().drop_count, 1);
        assert_eq!(cache.stats().evict_count, 1);
    }

    #[test]
    fn acquire_miss_increments_miss_count() {
        let mut cache: Cache<u32, u32, Lru, HashLookup<u32>> =
            Cache::builder().max_size(MaxSize::Count(2)).build();
        cache.entry(1).or_insert_with(|| 100);
        let _ = cache.acquire(&1); // hit
        let _ = cache.acquire(&999); // miss
        assert_eq!(cache.stats().hit_count, 1);
        assert_eq!(cache.stats().miss_count, 2); // entry()@1 + acquire(999)
    }

    #[test]
    fn disabled_cache_returns_detached_invalid_handle() {
        let mut cache: Cache<u32, u32, Lru, HashLookup<u32>> =
            Cache::builder().max_size(MaxSize::Disabled).build();
        let h = cache.entry(1).or_insert_with(|| 42);
        assert_eq!(*h, 42);
        assert!(h.is_invalid(), "detached handle must be invalid");
        assert_eq!(cache.len(), 0);
        assert!(cache.acquire(&1).is_none());
    }

    #[test]
    fn count_zero_cache_returns_detached_invalid_handle() {
        let mut cache: Cache<u32, u32, Lru, HashLookup<u32>> =
            Cache::builder().max_size(MaxSize::Count(0)).build();
        let h = cache.entry(1).or_insert_with(|| 7);
        assert_eq!(*h, 7);
        assert!(h.is_invalid(), "detached handle must be invalid");
        assert_eq!(cache.len(), 0);
        assert!(cache.acquire(&1).is_none());
    }

    #[test]
    #[should_panic(expected = "max_size must be configured")]
    fn builder_panics_without_max_size() {
        let _: Cache<u32, u32, Lru, HashLookup<u32>> = Cache::builder().build();
    }

    #[test]
    fn bytes_mode_evicts_lru_until_within_budget() {
        // budget = 24 bytes; each Vec<u32> entry is len*4 bytes.
        let mut cache: Cache<u32, alloc::vec::Vec<u32>, Lru, HashLookup<u32>> =
            Cache::builder().max_size(MaxSize::Bytes(24)).build();

        // 3 × 8-byte entries fit (24 bytes total).
        cache.entry(1).or_insert_with(|| alloc::vec![10u32, 11]);
        cache.entry(2).or_insert_with(|| alloc::vec![20u32, 21]);
        cache.entry(3).or_insert_with(|| alloc::vec![30u32, 31]);
        assert_eq!(cache.current_size(), 24);
        assert_eq!(cache.len(), 3);

        // Touch 1 so 2 becomes the LRU victim.
        let _ = cache.acquire(&1);

        // 4th entry forces evict; 2 leaves first.
        cache.entry(4).or_insert_with(|| alloc::vec![40u32, 41]);
        assert!(cache.acquire(&2).is_none(), "lru victim must be evicted");
        assert!(cache.acquire(&1).is_some());
        assert!(cache.acquire(&3).is_some());
        assert!(cache.acquire(&4).is_some());
        assert_eq!(cache.current_size(), 24);
    }

    #[test]
    fn bytes_mode_rejects_entry_larger_than_budget() {
        // 16-byte budget but inserting a 40-byte Vec<u32>.
        let mut cache: Cache<u32, alloc::vec::Vec<u32>, Lru, HashLookup<u32>> =
            Cache::builder().max_size(MaxSize::Bytes(16)).build();
        let h = cache
            .entry(1)
            .or_insert_with(|| alloc::vec![1u32, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        assert!(h.is_invalid(), "oversized entry must come back detached");
        assert_eq!(cache.len(), 0);
        assert_eq!(cache.current_size(), 0);
    }

    #[test]
    fn bytes_mode_zero_budget_rejects_everything() {
        let mut cache: Cache<u32, alloc::vec::Vec<u32>, Lru, HashLookup<u32>> =
            Cache::builder().max_size(MaxSize::Bytes(0)).build();
        let h = cache.entry(1).or_insert_with(|| alloc::vec![1u32]);
        assert!(h.is_invalid());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn bytes_mode_current_size_decreases_on_drop() {
        let mut cache: Cache<u32, alloc::vec::Vec<u32>, Lru, HashLookup<u32>> =
            Cache::builder().max_size(MaxSize::Bytes(64)).build();
        cache.entry(1).or_insert_with(|| alloc::vec![1u32, 2, 3]);
        cache.entry(2).or_insert_with(|| alloc::vec![4u32, 5]);
        assert_eq!(cache.current_size(), 20); // 12 + 8
        cache.drop(&1);
        assert_eq!(cache.current_size(), 8);
        cache.clear();
        assert_eq!(cache.current_size(), 0);
    }

    #[test]
    fn slab_id_reuse_keeps_lru_consistent() {
        let mut cache: Cache<u32, u32, Lru, HashLookup<u32>> =
            Cache::builder().max_size(MaxSize::Count(2)).build();
        for i in 0..10u32 {
            cache.entry(i).or_insert_with(|| i * 100);
        }
        assert!(cache.acquire(&8).is_some());
        assert!(cache.acquire(&9).is_some());
        assert!(cache.acquire(&7).is_none());
    }
}
