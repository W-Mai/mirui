use super::algorithm::Algorithm;
use super::core::{Cache, Entry};
use super::error::CacheError;
use super::handle::Handle;
use super::lookup::Lookup;

pub struct WithFactory<C, F> {
    cache: C,
    factory: F,
}

impl<K, V, A, L, F, E> WithFactory<Cache<K, V, A, L>, F>
where
    K: Clone,
    A: Algorithm,
    L: Lookup<K>,
    F: FnMut(&K) -> Result<V, E>,
{
    pub fn new(cache: Cache<K, V, A, L>, factory: F) -> Self {
        Self { cache, factory }
    }

    pub fn acquire_or_create(&mut self, key: K) -> Result<Handle<V>, CacheError<E>> {
        let factory_key = key.clone();
        match self.cache.entry(key) {
            Entry::Occupied(o) => Ok(o.into_handle()),
            Entry::Vacant(v) => {
                let value = (self.factory)(&factory_key).map_err(CacheError::Factory)?;
                Ok(v.insert(value))
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::{HashLookup, Lru, MaxSize};
    use core::cell::Cell;

    #[test]
    fn factory_runs_on_miss_only() {
        let calls = Cell::new(0u32);
        let cache: Cache<u32, alloc::string::String, Lru, HashLookup<u32>> =
            Cache::builder().max_size(MaxSize::Count(8)).build();
        let mut wrapped = WithFactory::new(cache, |k: &u32| -> Result<alloc::string::String, ()> {
            calls.set(calls.get() + 1);
            Ok(alloc::format!("v{k}"))
        });

        let h1 = wrapped.acquire_or_create(1).unwrap();
        let h2 = wrapped.acquire_or_create(1).unwrap();
        assert_eq!(&*h1, "v1");
        assert_eq!(&*h2, "v1");
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn factory_error_propagates() {
        let cache: Cache<u32, u32, Lru, HashLookup<u32>> =
            Cache::builder().max_size(MaxSize::Count(8)).build();
        let mut wrapped = WithFactory::new(cache, |_: &u32| -> Result<u32, &'static str> {
            Err("nope")
        });
        let r = wrapped.acquire_or_create(1);
        match r {
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
        wrapped.acquire_or_create(1).unwrap();
        wrapped.acquire_or_create(2).unwrap();
        let inner = wrapped.into_inner();
        assert_eq!(inner.len(), 2);
    }
}
