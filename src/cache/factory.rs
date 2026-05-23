use super::algorithm::Algorithm;
use super::budget::HasSize;
use super::core::{self, Cache};
use super::error::CacheError;
use super::handle::Handle;
use super::lookup::Lookup;

pub struct WithFactory<C, F> {
    cache: C,
    ctor: F,
}

impl<K, V, A, L, F> WithFactory<Cache<K, V, A, L>, F>
where
    K: Clone,
    A: Algorithm,
    L: Lookup<K>,
{
    pub fn new(cache: Cache<K, V, A, L>, ctor: F) -> Self {
        Self { cache, ctor }
    }

    pub fn acquire(&mut self, key: &K) -> Option<Handle<V>> {
        self.cache.acquire(key)
    }

    pub fn entry(&mut self, key: K) -> Entry<'_, K, V, A, L, F> {
        match self.cache.entry(key) {
            core::Entry::Occupied(o) => Entry::Occupied(OccupiedEntry {
                inner: o,
                _marker: ::core::marker::PhantomData,
            }),
            core::Entry::Vacant(v) => Entry::Vacant(VacantEntry {
                inner: v,
                ctor: &mut self.ctor,
            }),
        }
    }

    pub fn cache(&self) -> &Cache<K, V, A, L> {
        &self.cache
    }

    pub fn cache_mut(&mut self) -> &mut Cache<K, V, A, L> {
        &mut self.cache
    }

    pub fn into_inner(self) -> Cache<K, V, A, L> {
        self.cache
    }
}

pub enum Entry<'a, K, V, A, L, F>
where
    K: Clone,
    A: Algorithm,
    L: Lookup<K>,
{
    Occupied(OccupiedEntry<'a, K, V, A, L, F>),
    Vacant(VacantEntry<'a, K, V, A, L, F>),
}

pub struct OccupiedEntry<'a, K, V, A, L, F>
where
    K: Clone,
    A: Algorithm,
    L: Lookup<K>,
{
    inner: core::OccupiedEntry<'a, K, V, A, L>,
    _marker: ::core::marker::PhantomData<fn() -> F>,
}

pub struct VacantEntry<'a, K, V, A, L, F>
where
    K: Clone,
    A: Algorithm,
    L: Lookup<K>,
{
    inner: core::VacantEntry<'a, K, V, A, L>,
    ctor: &'a mut F,
}

impl<'a, K, V, A, L, F> Entry<'a, K, V, A, L, F>
where
    K: Clone,
    A: Algorithm,
    L: Lookup<K>,
{
    pub fn key(&self) -> &K {
        match self {
            Entry::Occupied(o) => o.inner.key(),
            Entry::Vacant(v) => v.inner.key(),
        }
    }
}

// Case 2: ctor takes only `&K`, no extra context.
impl<'a, K, V, A, L, F, E> Entry<'a, K, V, A, L, F>
where
    K: Clone,
    V: HasSize,
    A: Algorithm,
    L: Lookup<K>,
    F: FnMut(&K) -> Result<V, E>,
{
    pub fn or_insert(self) -> Result<Handle<V>, CacheError<E>> {
        match self {
            Entry::Occupied(o) => Ok(o.inner.into_handle()),
            Entry::Vacant(v) => {
                let value = (v.ctor)(v.inner.key()).map_err(CacheError::Factory)?;
                Ok(v.inner.insert(value))
            }
        }
    }
}

// Case 3: ctor signature is unconstrained; the build closure decides
// how to call it, typically capturing per-call ctx from the caller's
// scope and threading it into ctor.
impl<'a, K, V, A, L, F> Entry<'a, K, V, A, L, F>
where
    K: Clone,
    V: HasSize,
    A: Algorithm,
    L: Lookup<K>,
{
    pub fn or_insert_with<G, E>(self, build: G) -> Result<Handle<V>, CacheError<E>>
    where
        G: FnOnce(&mut F, &K) -> Result<V, E>,
    {
        match self {
            Entry::Occupied(o) => Ok(o.inner.into_handle()),
            Entry::Vacant(v) => {
                let value = build(v.ctor, v.inner.key()).map_err(CacheError::Factory)?;
                Ok(v.inner.insert(value))
            }
        }
    }
}

impl<'a, K, V, A, L, F> OccupiedEntry<'a, K, V, A, L, F>
where
    K: Clone,
    A: Algorithm,
    L: Lookup<K>,
{
    pub fn key(&self) -> &K {
        self.inner.key()
    }

    pub fn handle(&self) -> Handle<V> {
        self.inner.handle()
    }

    pub fn into_handle(self) -> Handle<V> {
        self.inner.into_handle()
    }
}

impl<'a, K, V, A, L, F> VacantEntry<'a, K, V, A, L, F>
where
    K: Clone,
    A: Algorithm,
    L: Lookup<K>,
{
    pub fn key(&self) -> &K {
        self.inner.key()
    }
}

impl<'a, K, V, A, L, F> VacantEntry<'a, K, V, A, L, F>
where
    K: Clone,
    V: HasSize,
    A: Algorithm,
    L: Lookup<K>,
{
    /// Insert a value directly, bypassing the cache's configured ctor.
    /// Useful when the caller has already produced V via some other
    /// path and just wants to register it under this key.
    pub fn insert(self, value: V) -> Handle<V> {
        self.inner.insert(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::{HashLookup, Lru, MaxSize};
    use ::core::cell::Cell;

    #[test]
    fn no_ctx_ctor_runs_on_miss_only() {
        let calls = Cell::new(0u32);
        let cache: Cache<u32, alloc::string::String, Lru, HashLookup<u32>> =
            Cache::builder().max_size(MaxSize::Count(8)).build();
        let mut wrapped = WithFactory::new(cache, |k: &u32| -> Result<alloc::string::String, ()> {
            calls.set(calls.get() + 1);
            Ok(alloc::format!("v{k}"))
        });

        let h1 = wrapped.entry(1).or_insert().unwrap();
        let h2 = wrapped.entry(1).or_insert().unwrap();
        assert_eq!(&*h1, "v1");
        assert_eq!(&*h2, "v1");
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn no_ctx_ctor_error_propagates() {
        let cache: Cache<u32, u32, Lru, HashLookup<u32>> =
            Cache::builder().max_size(MaxSize::Count(8)).build();
        let mut wrapped = WithFactory::new(cache, |_: &u32| -> Result<u32, &'static str> {
            Err("nope")
        });
        match wrapped.entry(1).or_insert() {
            Err(CacheError::Factory("nope")) => {}
            _ => panic!("expected Factory(\"nope\")"),
        }
        assert_eq!(wrapped.cache().len(), 0);
    }

    #[test]
    fn into_inner_returns_cache() {
        let cache: Cache<u32, u32, Lru, HashLookup<u32>> =
            Cache::builder().max_size(MaxSize::Count(2)).build();
        let mut wrapped = WithFactory::new(cache, |k: &u32| -> Result<u32, ()> { Ok(*k * 10) });
        wrapped.entry(1).or_insert().unwrap();
        wrapped.entry(2).or_insert().unwrap();
        let inner = wrapped.into_inner();
        assert_eq!(inner.len(), 2);
    }

    #[test]
    fn or_insert_with_threads_per_call_ctx_through_build_closure() {
        let log: Cell<u32> = Cell::new(0);
        let cache: Cache<u32, u32, Lru, HashLookup<u32>> =
            Cache::builder().max_size(MaxSize::Count(4)).build();
        let mut wrapped = WithFactory::new(cache, |k: &u32, multiplier: u32| -> Result<u32, ()> {
            log.set(log.get() + k * multiplier);
            Ok(*k * multiplier)
        });
        let h = wrapped
            .entry(3)
            .or_insert_with(|ctor, k| ctor(k, 10))
            .unwrap();
        assert_eq!(*h, 30);
        assert_eq!(log.get(), 30);
        let h2 = wrapped
            .entry(3)
            .or_insert_with(|ctor, k| ctor(k, 999))
            .unwrap();
        assert_eq!(*h2, 30);
        assert_eq!(log.get(), 30);
    }

    #[test]
    fn vacant_insert_bypasses_ctor() {
        let cache: Cache<u32, alloc::string::String, Lru, HashLookup<u32>> =
            Cache::builder().max_size(MaxSize::Count(4)).build();
        let mut wrapped = WithFactory::new(cache, |k: &u32| -> Result<alloc::string::String, ()> {
            Ok(alloc::format!("auto-{k}"))
        });
        let h = match wrapped.entry(1) {
            Entry::Occupied(o) => o.into_handle(),
            Entry::Vacant(v) => v.insert(alloc::string::String::from("manual")),
        };
        assert_eq!(&*h, "manual");
        let h2 = wrapped.entry(1).or_insert().unwrap();
        assert_eq!(&*h2, "manual");
    }

    #[test]
    fn acquire_only_queries() {
        let cache: Cache<u32, u32, Lru, HashLookup<u32>> =
            Cache::builder().max_size(MaxSize::Count(4)).build();
        let mut wrapped = WithFactory::new(cache, |k: &u32| -> Result<u32, ()> { Ok(*k * 10) });
        assert!(wrapped.acquire(&1).is_none());
        wrapped.entry(1).or_insert().unwrap();
        assert!(wrapped.acquire(&1).is_some());
    }
}
