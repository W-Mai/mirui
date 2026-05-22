use core::marker::PhantomData;

use super::algorithm::Algorithm;
use super::core::{Cache, Entry};
use super::error::CacheError;
use super::handle::Handle;
use super::lookup::Lookup;

pub struct WithFactory<C, F, Ctx = ()> {
    cache: C,
    factory: F,
    _ctx: PhantomData<fn(Ctx)>,
}

impl<K, V, A, L, F, E, Ctx> WithFactory<Cache<K, V, A, L>, F, Ctx>
where
    K: Clone,
    A: Algorithm,
    L: Lookup<K>,
    F: FnMut(&K, Ctx) -> Result<V, E>,
{
    pub fn new(cache: Cache<K, V, A, L>, factory: F) -> Self {
        Self {
            cache,
            factory,
            _ctx: PhantomData,
        }
    }

    pub fn acquire_or_create(&mut self, key: K, ctx: Ctx) -> Result<Handle<V>, CacheError<E>> {
        let factory_key = key.clone();
        match self.cache.entry(key) {
            Entry::Occupied(o) => Ok(o.into_handle()),
            Entry::Vacant(v) => {
                let value = (self.factory)(&factory_key, ctx).map_err(CacheError::Factory)?;
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
        let mut wrapped = WithFactory::new(
            cache,
            |k: &u32, _: ()| -> Result<alloc::string::String, ()> {
                calls.set(calls.get() + 1);
                Ok(alloc::format!("v{k}"))
            },
        );

        let h1 = wrapped.acquire_or_create(1, ()).unwrap();
        let h2 = wrapped.acquire_or_create(1, ()).unwrap();
        assert_eq!(&*h1, "v1");
        assert_eq!(&*h2, "v1");
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn factory_error_propagates() {
        let cache: Cache<u32, u32, Lru, HashLookup<u32>> =
            Cache::builder().max_size(MaxSize::Count(8)).build();
        let mut wrapped = WithFactory::new(cache, |_: &u32, _: ()| -> Result<u32, &'static str> {
            Err("nope")
        });
        let r = wrapped.acquire_or_create(1, ());
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
        let mut wrapped =
            WithFactory::new(cache, |k: &u32, _: ()| -> Result<u32, ()> { Ok(*k * 10) });
        wrapped.acquire_or_create(1, ()).unwrap();
        wrapped.acquire_or_create(2, ()).unwrap();
        let inner = wrapped.into_inner();
        assert_eq!(inner.len(), 2);
    }

    #[test]
    fn ctx_threads_per_call_data_through_to_factory() {
        let log: Cell<u32> = Cell::new(0);
        let cache: Cache<u32, u32, Lru, HashLookup<u32>> =
            Cache::builder().max_size(MaxSize::Count(4)).build();
        let mut wrapped = WithFactory::new(cache, |k: &u32, multiplier: u32| -> Result<u32, ()> {
            log.set(log.get() + k * multiplier);
            Ok(*k * multiplier)
        });
        let h = wrapped.acquire_or_create(3, 10).unwrap();
        assert_eq!(*h, 30);
        assert_eq!(log.get(), 30);
        let h2 = wrapped.acquire_or_create(3, 999).unwrap();
        assert_eq!(*h2, 30);
        assert_eq!(log.get(), 30);
    }
}
