use alloc::boxed::Box;
use alloc::vec::Vec;
use core::marker::PhantomData;

use hashbrown::HashMap;

use crate::cache::{Handle, HasSize, LruCache, MaxSize, UnboundCache};
use crate::state::Signal;

#[derive(Clone, Copy, Hash, Eq, PartialEq, Debug)]
pub struct ResourceHandle(u32);

pub struct Resource<T: 'static> {
    handle: ResourceHandle,
    // None = ready at creation (static/eager); Some = lazy, flipped true once loaded.
    ready: Option<Signal<bool>>,
    _phantom: PhantomData<T>,
}

impl<T: 'static> Clone for Resource<T> {
    fn clone(&self) -> Self {
        Resource {
            handle: self.handle,
            ready: self.ready.clone(),
            _phantom: PhantomData,
        }
    }
}

impl<T: 'static> Resource<T> {
    pub fn handle(&self) -> ResourceHandle {
        self.handle
    }

    // Reads the ready Signal inside an effect/render scope, so the caller subscribes.
    pub fn is_ready(&self) -> bool {
        self.ready.as_ref().is_none_or(|s| s.get())
    }
}

pub trait ResourceLoader<T> {
    fn can_load(&self, key: &str) -> bool;
    fn load(&self, key: &str) -> Option<T>;
}

type Factory<T> = Box<dyn FnOnce() -> T>;

pub struct ResourceManager<T: HasSize + 'static> {
    statics: UnboundCache<ResourceHandle, T>,
    cache: LruCache<ResourceHandle, T>,
    factories: HashMap<ResourceHandle, Factory<T>>,
    keys: HashMap<ResourceHandle, &'static str>,
    loaders: Vec<Box<dyn ResourceLoader<T>>>,
    by_key: HashMap<&'static str, ResourceHandle>,
    ready: HashMap<ResourceHandle, Signal<bool>>,
    next: u32,
}

impl<T: HasSize + 'static> ResourceManager<T> {
    pub fn new(budget: MaxSize) -> Self {
        ResourceManager {
            statics: UnboundCache::new(MaxSize::Unbound),
            cache: LruCache::new(budget),
            factories: HashMap::new(),
            keys: HashMap::new(),
            loaders: Vec::new(),
            by_key: HashMap::new(),
            ready: HashMap::new(),
            next: 0,
        }
    }

    fn alloc_handle(&mut self) -> ResourceHandle {
        let h = ResourceHandle(self.next);
        self.next += 1;
        h
    }

    pub fn register_loader<L: ResourceLoader<T> + 'static>(&mut self, loader: L) {
        self.loaders.push(Box::new(loader));
    }

    pub fn load_static(&mut self, key: &'static str, value: T) -> Resource<T> {
        let handle = self.alloc_handle();
        self.statics.entry(handle).or_insert_with(|| value);
        self.by_key.insert(key, handle);
        Resource {
            handle,
            ready: None,
            _phantom: PhantomData,
        }
    }

    pub fn load_lazy(
        &mut self,
        key: &'static str,
        factory: impl FnOnce() -> T + 'static,
    ) -> Resource<T> {
        let handle = self.alloc_handle();
        let ready = Signal::new(false);
        self.factories.insert(handle, Box::new(factory));
        self.ready.insert(handle, ready.clone());
        self.by_key.insert(key, handle);
        Resource {
            handle,
            ready: Some(ready),
            _phantom: PhantomData,
        }
    }

    pub fn load_keyed(&mut self, key: &'static str) -> Resource<T> {
        let handle = self.alloc_handle();
        let ready = Signal::new(false);
        self.keys.insert(handle, key);
        self.ready.insert(handle, ready.clone());
        self.by_key.insert(key, handle);
        Resource {
            handle,
            ready: Some(ready),
            _phantom: PhantomData,
        }
    }

    pub fn get(&mut self, h: ResourceHandle) -> Option<Handle<T>> {
        if let Some(handle) = self.statics.acquire(&h) {
            return Some(handle);
        }
        if let Some(handle) = self.cache.acquire(&h) {
            return Some(handle);
        }
        self.materialise(h)
    }

    pub fn evict(&mut self, h: ResourceHandle) {
        if self.cache.drop(&h) {
            if let Some(sig) = self.ready.get(&h) {
                sig.set(false);
            }
        }
    }

    fn materialise(&mut self, h: ResourceHandle) -> Option<Handle<T>> {
        let value = if let Some(factory) = self.factories.remove(&h) {
            factory()
        } else {
            let key = self.keys.get(&h)?;
            self.run_loaders(key)?
        };
        let handle = self.cache.entry(h).or_insert_with(|| value);
        if let Some(sig) = self.ready.get(&h) {
            sig.set(true);
        }
        Some(handle)
    }

    fn run_loaders(&self, key: &str) -> Option<T> {
        self.loaders
            .iter()
            .find(|l| l.can_load(key))
            .and_then(|l| l.load(key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::rc::Rc;
    use alloc::string::String;
    use core::cell::Cell;

    #[test]
    fn static_resource_reads_back() {
        let mut rm: ResourceManager<u32> = ResourceManager::new(MaxSize::Count(4));
        let r = rm.load_static("n", 42u32);
        assert!(r.is_ready());
        assert_eq!(rm.get(r.handle()).map(|h| *h), Some(42));
    }

    #[test]
    fn lazy_runs_factory_once_and_flips_ready() {
        let mut rm: ResourceManager<u32> = ResourceManager::new(MaxSize::Count(4));
        let runs = Rc::new(Cell::new(0));
        let runs2 = runs.clone();
        let r = rm.load_lazy("v", move || {
            runs2.set(runs2.get() + 1);
            7u32
        });
        assert!(!r.is_ready());
        assert_eq!(rm.get(r.handle()).map(|h| *h), Some(7));
        assert!(r.is_ready());
        assert_eq!(rm.get(r.handle()).map(|h| *h), Some(7));
        assert_eq!(runs.get(), 1);
    }

    #[test]
    fn evict_then_reresolve_runs_loader_again() {
        struct Loader;
        impl ResourceLoader<u32> for Loader {
            fn can_load(&self, key: &str) -> bool {
                key == "img"
            }
            fn load(&self, _key: &str) -> Option<u32> {
                Some(99u32)
            }
        }
        let mut rm: ResourceManager<u32> = ResourceManager::new(MaxSize::Count(4));
        rm.register_loader(Loader);
        let r = rm.load_keyed("img");
        assert_eq!(rm.get(r.handle()).map(|h| *h), Some(99));
        assert!(r.is_ready());
        rm.evict(r.handle());
        assert!(!r.is_ready());
        assert_eq!(rm.get(r.handle()).map(|h| *h), Some(99));
    }

    #[test]
    fn lru_evicts_oldest_static_stays() {
        let mut rm: ResourceManager<String> = ResourceManager::new(MaxSize::Count(2));
        let s = rm.load_static("s", String::from("keep"));
        let a = rm.load_lazy("a", || String::from("a"));
        let b = rm.load_lazy("b", || String::from("b"));
        let c = rm.load_lazy("c", || String::from("c"));
        assert_eq!(rm.get(a.handle()).as_deref().map(String::as_str), Some("a"));
        assert_eq!(rm.get(b.handle()).as_deref().map(String::as_str), Some("b"));
        assert_eq!(rm.get(c.handle()).as_deref().map(String::as_str), Some("c"));
        // `a` was the oldest evictable entry under Count(2); it fell out.
        assert!(rm.cache.acquire(&a.handle()).is_none());
        assert_eq!(
            rm.get(s.handle()).as_deref().map(String::as_str),
            Some("keep")
        );
    }

    #[test]
    fn handle_survives_eviction_as_invalid() {
        let mut rm: ResourceManager<u32> = ResourceManager::new(MaxSize::Count(4));
        let r = rm.load_lazy("v", || 5u32);
        let handle = rm.get(r.handle()).unwrap();
        assert!(!handle.is_invalid());
        rm.evict(r.handle());
        assert!(handle.is_invalid());
        assert_eq!(*handle, 5);
    }
}
