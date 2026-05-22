use super::algorithm::Algorithm;
use super::budget::MaxSize;
use super::core::Cache;
use super::lookup::Lookup;
use super::stats::CacheStats;

/// Type-erased read-only view over a single cache instance. Consumers
/// (registry, dump tools, plugins) iterate `&dyn CacheInspect` without
/// caring about the concrete `K / V / A / L` parameters.
pub trait CacheInspect: 'static {
    fn cache_name(&self) -> &'static str;
    fn cache_stats(&self) -> &CacheStats;
    fn cache_len(&self) -> usize;
    fn cache_max_size(&self) -> &MaxSize;
}

impl<K, V, A, L> CacheInspect for Cache<K, V, A, L>
where
    K: 'static,
    V: 'static,
    A: Algorithm + 'static,
    L: Lookup<K> + 'static,
{
    fn cache_name(&self) -> &'static str {
        self.name().unwrap_or("<unnamed>")
    }
    fn cache_stats(&self) -> &CacheStats {
        self.stats()
    }
    fn cache_len(&self) -> usize {
        self.len()
    }
    fn cache_max_size(&self) -> &MaxSize {
        self.max_size()
    }
}

/// One entry point over multiple caches owned by the same container
/// (e.g. a backend with label + shape caches). Consumers iterate once
/// instead of poking each cache field by name. Default returns an
/// empty iterator so backends without caches need no override.
pub trait InspectCaches {
    fn inspect_caches(&self) -> impl Iterator<Item = (&'static str, &dyn CacheInspect)> + '_ {
        core::iter::empty()
    }
}

/// Owned snapshot of a single cache's observable state. `World`
/// resource carries a `Vec` of these so plugins / systems can read
/// stats without holding a live borrow into the backend.
#[derive(Debug, Clone, Copy)]
pub struct CacheStatsSnapshot {
    pub name: &'static str,
    pub stats: CacheStats,
    pub len: usize,
    pub max_size: MaxSize,
}

impl CacheStatsSnapshot {
    pub fn capture(cache: &dyn CacheInspect) -> Self {
        Self {
            name: cache.cache_name(),
            stats: *cache.cache_stats(),
            len: cache.cache_len(),
            max_size: *cache.cache_max_size(),
        }
    }
}

/// World resource: latest cache snapshots, refreshed by `App::run`
/// each frame between `FrameTimings` publish and `post_render`.
#[derive(Debug, Default)]
pub struct CacheRegistry {
    snapshots: alloc::vec::Vec<CacheStatsSnapshot>,
}

impl CacheRegistry {
    pub(crate) fn from_snapshots(snapshots: alloc::vec::Vec<CacheStatsSnapshot>) -> Self {
        Self { snapshots }
    }

    pub fn snapshots(&self) -> &[CacheStatsSnapshot] {
        &self.snapshots
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::{HashLookup, Lru, MaxSize as M};
    use alloc::vec::Vec;

    #[test]
    fn cache_inspect_reports_name_stats_len_max() {
        let mut cache: Cache<u32, u32, Lru, HashLookup<u32>> = Cache::builder()
            .max_size(M::Count(8))
            .name("test_cache")
            .build();
        cache.entry(1).or_insert_with(|| 10);

        let view: &dyn CacheInspect = &cache;
        assert_eq!(view.cache_name(), "test_cache");
        assert_eq!(view.cache_len(), 1);
        assert_eq!(*view.cache_max_size(), M::Count(8));
        assert_eq!(view.cache_stats().insert_count, 1);
    }

    #[test]
    fn unnamed_cache_falls_back() {
        let cache: Cache<u32, u32, Lru, HashLookup<u32>> =
            Cache::builder().max_size(M::Count(2)).build();
        let view: &dyn CacheInspect = &cache;
        assert_eq!(view.cache_name(), "<unnamed>");
    }

    #[test]
    fn inspect_caches_walks_a_collection() {
        struct Two<A, B>(A, B);
        impl<A: CacheInspect, B: CacheInspect> InspectCaches for Two<A, B> {
            fn inspect_caches(
                &self,
            ) -> impl Iterator<Item = (&'static str, &dyn CacheInspect)> + '_ {
                [("first", &self.0 as &dyn CacheInspect), ("second", &self.1)].into_iter()
            }
        }

        let c1: Cache<u32, u32, Lru, HashLookup<u32>> =
            Cache::builder().max_size(M::Count(2)).name("c1").build();
        let c2: Cache<u32, u32, Lru, HashLookup<u32>> =
            Cache::builder().max_size(M::Count(4)).name("c2").build();
        let two = Two(c1, c2);

        let names: Vec<_> = two.inspect_caches().map(|(n, _)| n).collect();
        assert_eq!(names, ["first", "second"]);
    }
}
