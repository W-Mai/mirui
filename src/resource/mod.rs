use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::vec::Vec;
use core::any::Any;
use core::marker::PhantomData;

use hashbrown::HashMap;

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

pub trait ResourceLoader {
    fn can_load(&self, key: &str) -> bool;
    fn load(&self, key: &str) -> Option<Box<dyn Any>>;
}

enum Slot {
    Static(Box<dyn Any>),
    Lazy {
        factory: Option<Box<dyn FnOnce() -> Box<dyn Any>>>,
        val: Option<Box<dyn Any>>,
    },
    Loaded {
        key: &'static str,
        val: Option<Box<dyn Any>>,
    },
}

struct Entry {
    slot: Slot,
    ready: Option<Signal<bool>>,
    evictable: bool,
}

pub struct ResourceManager {
    by_handle: HashMap<ResourceHandle, Entry>,
    by_key: HashMap<&'static str, ResourceHandle>,
    loaders: Vec<Box<dyn ResourceLoader>>,
    // front = least-recently-used; only evictable handles ever enter.
    lru: VecDeque<ResourceHandle>,
    max_resident: usize,
    next: u32,
}

impl ResourceManager {
    pub fn new(max_resident: usize) -> Self {
        ResourceManager {
            by_handle: HashMap::new(),
            by_key: HashMap::new(),
            loaders: Vec::new(),
            lru: VecDeque::new(),
            max_resident,
            next: 0,
        }
    }

    fn alloc_handle(&mut self) -> ResourceHandle {
        let h = ResourceHandle(self.next);
        self.next += 1;
        h
    }

    pub fn register_loader<L: ResourceLoader + 'static>(&mut self, loader: L) {
        self.loaders.push(Box::new(loader));
    }

    pub fn load_static<T: 'static>(&mut self, key: &'static str, value: T) -> Resource<T> {
        let handle = self.alloc_handle();
        self.by_handle.insert(
            handle,
            Entry {
                slot: Slot::Static(Box::new(value)),
                ready: None,
                evictable: false,
            },
        );
        self.by_key.insert(key, handle);
        Resource {
            handle,
            ready: None,
            _phantom: PhantomData,
        }
    }

    pub fn load_keyed<T: 'static>(&mut self, key: &'static str) -> Resource<T> {
        let handle = self.alloc_handle();
        let ready = Signal::new(false);
        self.by_handle.insert(
            handle,
            Entry {
                slot: Slot::Loaded { key, val: None },
                ready: Some(ready.clone()),
                evictable: true,
            },
        );
        self.by_key.insert(key, handle);
        Resource {
            handle,
            ready: Some(ready),
            _phantom: PhantomData,
        }
    }

    pub fn load_lazy<T: 'static>(
        &mut self,
        key: &'static str,
        factory: impl FnOnce() -> T + 'static,
    ) -> Resource<T> {
        let handle = self.alloc_handle();
        let ready = Signal::new(false);
        let boxed: Box<dyn FnOnce() -> Box<dyn Any>> =
            Box::new(move || Box::new(factory()) as Box<dyn Any>);
        self.by_handle.insert(
            handle,
            Entry {
                slot: Slot::Lazy {
                    factory: Some(boxed),
                    val: None,
                },
                ready: Some(ready.clone()),
                evictable: true,
            },
        );
        self.by_key.insert(key, handle);
        Resource {
            handle,
            ready: Some(ready),
            _phantom: PhantomData,
        }
    }

    // Value never escapes the manager, so a later eviction can't dangle a reference.
    pub fn with<T: 'static, R>(&mut self, h: Resource<T>, f: impl FnOnce(&T) -> R) -> Option<R> {
        self.resolve(h.handle);
        self.touch(h.handle);
        let entry = self.by_handle.get(&h.handle)?;
        let any = entry.slot.value()?;
        any.downcast_ref::<T>().map(f)
    }

    pub fn get_cloned<T: 'static + Clone>(&mut self, h: Resource<T>) -> Option<T> {
        self.with(h, |v| v.clone())
    }

    pub fn evict(&mut self, h: ResourceHandle) {
        let Some(entry) = self.by_handle.get_mut(&h) else {
            return;
        };
        // Drop the materialised value but keep the factory so a later read re-runs it.
        match &mut entry.slot {
            Slot::Lazy { val, .. } | Slot::Loaded { val, .. } => *val = None,
            Slot::Static(_) => return,
        }
        if let Some(sig) = &entry.ready {
            sig.set(false);
        }
        self.lru.retain(|&x| x != h);
    }

    fn resolve(&mut self, h: ResourceHandle) {
        let Some(entry) = self.by_handle.get_mut(&h) else {
            return;
        };
        // run_loaders needs &self.loaders, can't coexist with &mut entry — detach first.
        enum Pending {
            Factory(Box<dyn FnOnce() -> Box<dyn Any>>),
            Loader(&'static str),
            None,
        }
        let pending = match &mut entry.slot {
            Slot::Lazy { factory, val } if val.is_none() => {
                factory.take().map_or(Pending::None, Pending::Factory)
            }
            Slot::Loaded { key, val } if val.is_none() => Pending::Loader(key),
            _ => Pending::None,
        };
        let produced = match pending {
            Pending::Factory(f) => Some(f()),
            Pending::Loader(key) => self.run_loaders(key),
            Pending::None => return,
        };
        let entry = self.by_handle.get_mut(&h).unwrap();
        match &mut entry.slot {
            Slot::Lazy { val, .. } | Slot::Loaded { val, .. } => *val = produced,
            Slot::Static(_) => {}
        }
        if let Some(sig) = &entry.ready {
            sig.set(true);
        }
    }

    fn run_loaders(&self, key: &str) -> Option<Box<dyn Any>> {
        self.loaders
            .iter()
            .find(|l| l.can_load(key))
            .and_then(|l| l.load(key))
    }

    fn touch(&mut self, h: ResourceHandle) {
        let evictable = self.by_handle.get(&h).map(|e| e.evictable).unwrap_or(false);
        if !evictable {
            return;
        }
        self.lru.retain(|&x| x != h);
        self.lru.push_back(h);
        while self.lru.len() > self.max_resident {
            if let Some(old) = self.lru.pop_front() {
                self.evict(old);
            }
        }
    }
}

impl Slot {
    fn value(&self) -> Option<&dyn Any> {
        match self {
            Slot::Static(v) => Some(v.as_ref()),
            Slot::Lazy { val, .. } | Slot::Loaded { val, .. } => val.as_deref(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::rc::Rc;
    use core::cell::Cell;

    #[test]
    fn static_resource_reads_back() {
        let mut rm = ResourceManager::new(4);
        let r = rm.load_static("n", 42u32);
        assert!(r.is_ready());
        assert_eq!(rm.get_cloned(r), Some(42));
    }

    #[test]
    fn lazy_runs_factory_once_and_flips_ready() {
        let mut rm = ResourceManager::new(4);
        let runs = Rc::new(Cell::new(0));
        let runs2 = runs.clone();
        let r = rm.load_lazy("v", move || {
            runs2.set(runs2.get() + 1);
            7u32
        });
        assert!(!r.is_ready());
        assert_eq!(rm.get_cloned(r.clone()), Some(7));
        assert!(r.is_ready());
        assert_eq!(rm.get_cloned(r.clone()), Some(7));
        assert_eq!(runs.get(), 1);
    }

    #[test]
    fn evict_then_reresolve_runs_loader_again() {
        struct Loader;
        impl ResourceLoader for Loader {
            fn can_load(&self, key: &str) -> bool {
                key == "img"
            }
            fn load(&self, _key: &str) -> Option<Box<dyn Any>> {
                Some(Box::new(99u32))
            }
        }
        let mut rm = ResourceManager::new(4);
        rm.register_loader(Loader);
        let r = rm.load_keyed::<u32>("img");
        assert_eq!(rm.get_cloned(r.clone()), Some(99));
        assert!(r.is_ready());
        rm.evict(r.handle());
        assert!(!r.is_ready());
        assert_eq!(rm.get_cloned(r), Some(99));
    }

    #[test]
    fn lru_evicts_oldest_static_stays() {
        let mut rm = ResourceManager::new(2);
        let s = rm.load_static("s", 1u32);
        let a = rm.load_lazy("a", || 10u32);
        let b = rm.load_lazy("b", || 20u32);
        let c = rm.load_lazy("c", || 30u32);
        assert_eq!(rm.get_cloned(a.clone()), Some(10));
        assert_eq!(rm.get_cloned(b.clone()), Some(20));
        assert_eq!(rm.get_cloned(c), Some(30));
        assert!(!a.is_ready());
        assert!(b.is_ready());
        assert_eq!(rm.get_cloned(s), Some(1));
    }

    #[test]
    fn wrong_type_downcast_is_none() {
        let mut rm = ResourceManager::new(4);
        let r = rm.load_static("n", 42u32);
        assert!(
            rm.with(
                Resource::<i64> {
                    handle: r.handle(),
                    ready: None,
                    _phantom: PhantomData
                },
                |_| ()
            )
            .is_none()
        );
    }
}
