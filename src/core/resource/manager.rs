use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::rc::Rc;
use core::any::Any;
use core::cell::RefCell;

use hashbrown::HashSet;

use crate::core::cache::{HasSize, LruCache, MaxSize};
use crate::core::reactive::Signal;
use crate::core::resource::handle::ResourceHandle;
use crate::core::resource::loader::Loader;
use crate::core::resource::manager_inner::{self, Entry, EntryKind, ManagerInner, ResolveOutcome};
use crate::core::resource::probe::HasProbe;

/// Optional probe state held alongside [`ManagerInner`]. The user opts in by
/// calling [`ResourceManager::enable_probes`] (or [`ResourceManager::with_probes`]).
/// Stored as `Box<dyn Any>` inside an `Option` so non-`HasProbe` `T` types
/// don't drag `T::Meta` into [`ResourceManager`].
struct ProbeSidecar<T: HasProbe + 'static> {
    probes: LruCache<Cow<'static, str>, T::Meta>,
    fallback_meta: T::Meta,
    failed_probes: HashSet<Cow<'static, str>>,
}

pub struct ResourceManager<T: HasSize + Clone + 'static> {
    inner: Rc<RefCell<ManagerInner<T>>>,
    probe_sidecar: RefCell<Option<Box<dyn Any>>>,
}

impl<T: HasSize + Clone + 'static> ResourceManager<T> {
    pub fn new(values_budget: MaxSize, fallback: T) -> Self {
        Self {
            inner: Rc::new(RefCell::new(ManagerInner::new(values_budget, fallback))),
            probe_sidecar: RefCell::new(None),
        }
    }

    /// Chain-style alias for `set_fallback` that consumes self. The two
    /// forms encode lifecycle: `with_*` for construction-time chaining
    /// (`ResourceManager::new(...).with_fallback(...)`), `set_*`/`add_*`
    /// for `&self` runtime mutation after the manager is in World.
    pub fn with_fallback(self, value: T) -> Self {
        self.set_fallback(value);
        self
    }

    pub fn with_static(self, token: impl Into<Cow<'static, str>>, value: T) -> Self {
        self.add_static(token, value);
        self
    }

    pub fn with_factory(
        self,
        token: impl Into<Cow<'static, str>>,
        factory: impl FnOnce() -> Option<T> + 'static,
    ) -> Self {
        self.add_factory(token, factory);
        self
    }

    pub fn with_loader(self, loader: impl Loader<T>) -> Self {
        self.add_loader(loader);
        self
    }

    pub fn set_fallback(&self, value: T) {
        self.inner.borrow_mut().set_fallback(value);
    }

    pub fn add_static(&self, token: impl Into<Cow<'static, str>>, value: T) {
        self.inner
            .borrow_mut()
            .insert_entry(token.into(), Entry::Static(value));
    }

    pub fn add_factory(
        &self,
        token: impl Into<Cow<'static, str>>,
        factory: impl FnOnce() -> Option<T> + 'static,
    ) {
        self.inner
            .borrow_mut()
            .insert_entry(token.into(), Entry::Factory(Box::new(factory)));
    }

    pub fn add_loader(&self, loader: impl Loader<T>) {
        self.inner.borrow_mut().add_loader(Box::new(loader));
    }

    pub fn remove_token(&self, token: &str) {
        self.inner.borrow_mut().unregister(token);
    }

    pub fn load(&self, token: impl Into<Cow<'static, str>>) -> ResourceHandle<T> {
        let token: Cow<'static, str> = token.into();
        let fallback = {
            let mut inner = self.inner.borrow_mut();
            inner.bump_refcount(&token);
            inner.fallback_clone()
        };
        ResourceHandle::new(token, Rc::downgrade(&self.inner), fallback)
    }

    pub fn resolve(&self, token: &str) -> Rc<T> {
        match manager_inner::resolve(&self.inner, token) {
            ResolveOutcome::CacheHit(rc) => rc,
            ResolveOutcome::JustResolved {
                value,
                precomputed_probe,
            } => {
                self.try_mirror_probe(token, &value, precomputed_probe);
                value
            }
            ResolveOutcome::Fallback(rc) => rc,
        }
    }

    pub fn prefetch(&self, token: &str) {
        let _ = self.resolve(token);
    }

    pub fn subscribe(&self, token: &str) -> Rc<Signal<()>> {
        self.inner.borrow_mut().signal_for(Cow::Owned(token.into()))
    }

    pub fn clear_failed(&self, token: &str) {
        self.inner.borrow_mut().clear_failed(token);
    }

    pub fn clear_all_failed(&self) {
        self.inner.borrow_mut().clear_all_failed();
    }

    // Default no-op: when T: !HasProbe the sidecar is always None. The
    // `T: HasProbe` impl block overrides this method so resolved values
    // get mirrored into the probes cache.
    fn try_mirror_probe(&self, _token: &str, _value: &T, _precomputed: Option<Box<dyn Any>>) {}
}

/// Probe-aware API gated on `T: HasProbe`. These methods are only visible
/// when the resource type opts into [`HasProbe`]. The probe sidecar is
/// initialised by [`ResourceManager::enable_probes`] (or
/// [`ResourceManager::with_probes`]) — until that's called, `probe()`
/// returns `None` for every token.
impl<T: HasSize + HasProbe + Clone + 'static> ResourceManager<T> {
    /// Chain-style alias for `enable_probes` (init-time variant). See the
    /// `with_fallback` doc on the generic impl for the lifecycle convention.
    pub fn with_probes(self, probes_budget: MaxSize, fallback_meta: T::Meta) -> Self {
        self.enable_probes(probes_budget, fallback_meta);
        self
    }

    pub fn with_probed_factory(
        self,
        token: impl Into<Cow<'static, str>>,
        probe: T::Meta,
        factory: impl FnOnce() -> Option<T> + 'static,
    ) -> Self {
        self.add_probed_factory(token, probe, factory);
        self
    }

    pub fn enable_probes(&self, probes_budget: MaxSize, fallback_meta: T::Meta) {
        let sidecar = ProbeSidecar::<T> {
            probes: LruCache::new(probes_budget),
            fallback_meta,
            failed_probes: HashSet::new(),
        };
        *self.probe_sidecar.borrow_mut() = Some(Box::new(sidecar));
    }

    pub fn set_probes_budget(&self, max: MaxSize) {
        self.with_sidecar_mut(|side| {
            // cache::Cache has no in-place resize, so swap in a fresh one.
            let kept = side.fallback_meta.clone();
            side.probes = LruCache::new(max);
            side.fallback_meta = kept;
        });
    }

    pub fn set_fallback_meta(&self, meta: T::Meta) {
        self.with_sidecar_mut(|side| side.fallback_meta = meta);
    }

    pub fn add_probed_factory(
        &self,
        token: impl Into<Cow<'static, str>>,
        probe: T::Meta,
        factory: impl FnOnce() -> Option<T> + 'static,
    ) {
        self.inner.borrow_mut().insert_entry(
            token.into(),
            Entry::FactoryWithProbe {
                probe: Box::new(probe),
                factory: Box::new(factory),
            },
        );
    }

    pub fn probe(&self, token: &str) -> Option<T::Meta> {
        // Step 1: probes cache hit / failed-set. The closure returns
        // `Some(answer)` to short-circuit, `None` to fall through to step 3.
        // The outer `Option<...>` from with_sidecar_mut is None when the
        // sidecar isn't installed at all — also treated as fall-through.
        if let Some(Some(answer)) = self.with_sidecar_mut(|side| {
            if let Some(h) = side.probes.acquire(token) {
                return Some(Some((*h).clone()));
            }
            if side.failed_probes.contains(token) {
                return Some(None);
            }
            None
        }) {
            return answer;
        }

        // Step 3: values cache hit → derive meta and mirror it forward
        if let Some(rc) = self.inner.borrow_mut().try_acquire_value_clone(token) {
            let meta = (*rc).extract_meta();
            self.insert_probe(token, meta.clone());
            return Some(meta);
        }

        // Step 4: peek by_token kind (no borrow held across the dispatch)
        let entry_kind = self.inner.borrow().peek_entry_kind(token);
        match entry_kind {
            Some(EntryKind::Static) => {
                let entry = self.inner.borrow_mut().take_entry(token);
                if let Some(Entry::Static(v)) = entry {
                    let meta = v.extract_meta();
                    self.inner
                        .borrow_mut()
                        .insert_entry(Cow::Owned(token.into()), Entry::Static(v));
                    self.insert_probe(token, meta.clone());
                    return Some(meta);
                }
                return None;
            }
            Some(EntryKind::FactoryWithProbe) => {
                // Take the entry out so we can read its probe payload, then
                // put it back with a fresh probe clone so the next call still
                // hits the same cheap path.
                let entry = self.inner.borrow_mut().take_entry(token)?;
                let meta: T::Meta = entry
                    .factory_probe()
                    .expect("FactoryWithProbe::probe type mismatch");
                let Entry::FactoryWithProbe { factory, .. } = entry else {
                    unreachable!("EntryKind::FactoryWithProbe must match Entry::FactoryWithProbe");
                };
                self.inner.borrow_mut().insert_entry(
                    Cow::Owned(token.into()),
                    Entry::FactoryWithProbe {
                        probe: Box::new(meta.clone()),
                        factory,
                    },
                );
                self.insert_probe(token, meta.clone());
                return Some(meta);
            }
            Some(EntryKind::Factory) => {
                // Plain Factory cannot answer probe cheaply. Mark failed.
                self.mark_failed_probe(token);
                return None;
            }
            None => {}
        }

        // Step 5: loader chain
        match manager_inner::walk_probe_chain::<T>(&self.inner, token) {
            Some(Ok(meta)) => {
                self.insert_probe(token, meta.clone());
                Some(meta)
            }
            Some(Err(_)) | None => {
                self.mark_failed_probe(token);
                None
            }
        }
    }

    pub fn prefetch_probe(&self, token: &str) {
        let _ = self.probe(token);
    }

    pub fn fallback_meta(&self) -> Option<T::Meta> {
        self.with_sidecar(|side| side.fallback_meta.clone())
    }

    fn insert_probe(&self, token: &str, meta: T::Meta) {
        self.with_sidecar_mut(|side| {
            let _ = side
                .probes
                .entry(Cow::Owned(token.into()))
                .or_insert_with(|| meta);
        });
    }

    fn mark_failed_probe(&self, token: &str) {
        self.with_sidecar_mut(|side| {
            side.failed_probes.insert(Cow::Owned(token.into()));
        });
    }

    fn with_sidecar<R>(&self, f: impl FnOnce(&ProbeSidecar<T>) -> R) -> Option<R> {
        let guard = self.probe_sidecar.borrow();
        let side_box = guard.as_ref()?;
        let side = side_box.downcast_ref::<ProbeSidecar<T>>()?;
        Some(f(side))
    }

    fn with_sidecar_mut<R>(&self, f: impl FnOnce(&mut ProbeSidecar<T>) -> R) -> Option<R> {
        let mut guard = self.probe_sidecar.borrow_mut();
        let side_box = guard.as_mut()?;
        let side = side_box.downcast_mut::<ProbeSidecar<T>>()?;
        Some(f(side))
    }
}
