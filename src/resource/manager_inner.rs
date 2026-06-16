use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::vec::Vec;
use core::any::Any;
use core::cell::RefCell;

use hashbrown::{HashMap, HashSet};

use crate::cache::{HasSize, LruCache, MaxSize};
use crate::resource::loader::{LoadError, Loader, ProbeLoader};
use crate::resource::probe::HasProbe;
use crate::state::Signal;

/// Per-token registration. `FactoryWithProbe` carries a precomputed probe
/// blob so `probe()` can answer without ever running the factory; the blob
/// is type-erased here so `Entry` stays usable for `T: !HasProbe` too.
pub(crate) enum Entry<T: 'static> {
    Static(T),
    Factory(Box<dyn FnOnce() -> Option<T>>),
    FactoryWithProbe {
        probe: Box<dyn Any>,
        factory: Box<dyn FnOnce() -> Option<T>>,
    },
}

/// Variant tag without payload, returned by [`ManagerInner::peek_entry_kind`]
/// so callers can dispatch on shape without holding a borrow into [`ManagerInner`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EntryKind {
    Static,
    Factory,
    FactoryWithProbe,
}

impl<T: 'static> Entry<T> {
    pub(crate) fn kind(&self) -> EntryKind {
        match self {
            Entry::Static(_) => EntryKind::Static,
            Entry::Factory(_) => EntryKind::Factory,
            Entry::FactoryWithProbe { .. } => EntryKind::FactoryWithProbe,
        }
    }

    // Pull the precomputed probe blob out of a FactoryWithProbe variant and
    // downcast it to M. Returns None for any other variant or a type mismatch.
    pub(crate) fn factory_probe<M: Clone + 'static>(&self) -> Option<M> {
        if let Entry::FactoryWithProbe { probe, .. } = self {
            probe.downcast_ref::<M>().cloned()
        } else {
            None
        }
    }
}

pub struct ManagerInner<T: HasSize + Clone + 'static> {
    values: LruCache<Cow<'static, str>, T>,
    by_token: HashMap<Cow<'static, str>, Entry<T>>,
    loaders: Vec<Box<dyn Loader<T>>>,
    fallback: Rc<T>,
    failed_values: HashSet<Cow<'static, str>>,
    signals: HashMap<Cow<'static, str>, Rc<Signal<()>>>,
    refcounts: HashMap<Cow<'static, str>, u32>,
}

impl<T: HasSize + Clone + 'static> ManagerInner<T> {
    pub(crate) fn new(values_budget: MaxSize, fallback: T) -> Self {
        Self {
            values: LruCache::new(values_budget),
            by_token: HashMap::new(),
            loaders: Vec::new(),
            fallback: Rc::new(fallback),
            failed_values: HashSet::new(),
            signals: HashMap::new(),
            refcounts: HashMap::new(),
        }
    }

    // ---- fallback / loaders / refcount / signals ----

    pub(crate) fn set_fallback(&mut self, value: T) {
        self.fallback = Rc::new(value);
    }

    pub(crate) fn fallback_clone(&self) -> Rc<T> {
        self.fallback.clone()
    }

    pub(crate) fn add_loader(&mut self, loader: Box<dyn Loader<T>>) {
        self.loaders.push(loader);
    }

    pub(crate) fn bump_refcount(&mut self, token: &str) {
        *self.refcounts.entry(Cow::Owned(token.into())).or_insert(0) += 1;
    }

    pub(crate) fn drop_refcount(&mut self, token: &str) {
        if let Some(c) = self.refcounts.get_mut(token) {
            *c = c.saturating_sub(1);
            if *c == 0 {
                self.refcounts.remove(token);
            }
        }
    }

    pub(crate) fn signal_for(&mut self, token: Cow<'static, str>) -> Rc<Signal<()>> {
        self.signals
            .entry(token)
            .or_insert_with(|| Rc::new(Signal::new(())))
            .clone()
    }

    fn notify(&self, token: &str) {
        if let Some(sig) = self.signals.get(token) {
            sig.set(());
        }
    }

    // ---- entry registration / removal ----

    pub(crate) fn insert_entry(&mut self, token: Cow<'static, str>, entry: Entry<T>) {
        self.by_token.insert(token, entry);
    }

    pub(crate) fn take_entry(&mut self, token: &str) -> Option<Entry<T>> {
        self.by_token.remove(token)
    }

    pub(crate) fn peek_entry_kind(&self, token: &str) -> Option<EntryKind> {
        self.by_token.get(token).map(Entry::kind)
    }

    pub(crate) fn unregister(&mut self, token: &str) {
        self.by_token.remove(token);
        self.failed_values.remove(token);
        self.values.drop(&Cow::Owned(token.into()));
        self.signals.remove(token);
    }

    // ---- failed-set ----

    pub(crate) fn mark_failed(&mut self, token: &str) {
        self.failed_values.insert(Cow::Owned(token.into()));
    }

    pub(crate) fn clear_failed(&mut self, token: &str) {
        self.failed_values.remove(token);
    }

    pub(crate) fn clear_all_failed(&mut self) {
        self.failed_values.clear();
    }

    pub(crate) fn is_failed(&self, token: &str) -> bool {
        self.failed_values.contains(token)
    }

    // ---- value cache helpers ----

    // Acquire a token from the values cache and clone it into an owned Rc<T>
    // for the caller. Returns None if there's no cached value.
    pub(crate) fn try_acquire_value_clone(&mut self, token: &str) -> Option<Rc<T>> {
        let h = self.values.acquire(&Cow::Owned(token.into()))?;
        Some(Rc::new((*h).clone()))
    }

    // Insert `value` into the values cache (or return the already-cached
    // entry if a concurrent path beat us). Notifies the per-token signal.
    pub(crate) fn insert_value(&mut self, token: Cow<'static, str>, value: T) -> Rc<T> {
        let handle = self.values.entry(token.clone()).or_insert_with(|| value);
        let rc = Rc::new((*handle).clone());
        self.notify(&token);
        rc
    }
}

pub(crate) enum ResolveOutcome<T> {
    CacheHit(Rc<T>),
    JustResolved {
        value: Rc<T>,
        precomputed_probe: Option<Box<dyn Any>>,
    },
    Fallback(Rc<T>),
}

pub(crate) fn resolve<T: HasSize + Clone + 'static>(
    rc: &Rc<RefCell<ManagerInner<T>>>,
    token: &str,
) -> ResolveOutcome<T> {
    // Step 1: values cache hit
    {
        let mut inner = rc.borrow_mut();
        if let Some(v) = inner.try_acquire_value_clone(token) {
            return ResolveOutcome::CacheHit(v);
        }
        // Step 2: known-failed
        if inner.is_failed(token) {
            return ResolveOutcome::Fallback(inner.fallback_clone());
        }
    }

    // Step 3: by_token
    let entry_taken = rc.borrow_mut().take_entry(token);
    if let Some(entry) = entry_taken {
        let (value, precomputed_probe) = match entry {
            Entry::Static(v) => (Some(v), None),
            Entry::Factory(f) => (f(), None),
            Entry::FactoryWithProbe { probe, factory } => (factory(), Some(probe)),
        };
        return finish::<T>(rc, token, value, precomputed_probe);
    }

    // Step 4: loader chain. Walk through loaders and, on the first claim, pull
    // the value out without holding a borrow across the loader call (so loaders
    // can call back into the manager without triggering a re-entrant
    // borrow_mut panic).
    let chain_outcome = walk_load_chain(rc, token);
    match chain_outcome {
        ChainOutcome::Loaded(v) => finish::<T>(rc, token, Some(v), None),
        ChainOutcome::Failed(_) | ChainOutcome::AllNotMine => {
            let mut inner = rc.borrow_mut();
            inner.mark_failed(token);
            ResolveOutcome::Fallback(inner.fallback_clone())
        }
    }
}

enum ChainOutcome<T> {
    Loaded(T),
    #[allow(dead_code)] // message is surfaced at the LoadError::Failed boundary
    Failed(&'static str),
    AllNotMine,
}

fn walk_load_chain<T: HasSize + Clone + 'static>(
    rc: &Rc<RefCell<ManagerInner<T>>>,
    token: &str,
) -> ChainOutcome<T> {
    let n = rc.borrow().loaders.len();
    for i in 0..n {
        // Hold the borrow only while reading the trait-object slot so a
        // loader that re-enters the manager from try_load doesn't trip a
        // RefCell already-borrowed panic.
        let outcome = {
            let inner = rc.borrow();
            inner.loaders[i].try_load(token)
        };
        match outcome {
            Ok(v) => return ChainOutcome::Loaded(v),
            Err(LoadError::NotMine) => continue,
            Err(LoadError::Failed(msg)) => return ChainOutcome::Failed(msg),
        }
    }
    ChainOutcome::AllNotMine
}

fn finish<T: HasSize + Clone + 'static>(
    rc: &Rc<RefCell<ManagerInner<T>>>,
    token: &str,
    value: Option<T>,
    precomputed_probe: Option<Box<dyn Any>>,
) -> ResolveOutcome<T> {
    let token_owned: Cow<'static, str> = Cow::Owned(token.into());
    let Some(v) = value else {
        let mut inner = rc.borrow_mut();
        inner.mark_failed(token);
        return ResolveOutcome::Fallback(inner.fallback_clone());
    };

    let value_rc = rc.borrow_mut().insert_value(token_owned, v);
    ResolveOutcome::JustResolved {
        value: value_rc,
        precomputed_probe,
    }
}

/// Walk the loader chain calling [`ProbeLoader::try_probe`] on each. Lives
/// here next to [`ManagerInner`] so it can iterate `loaders` without exposing
/// the field, and requires `T: HasProbe` so it only exists for probe-capable
/// resources.
pub(crate) fn walk_probe_chain<T: HasSize + HasProbe + Clone + 'static>(
    rc: &Rc<RefCell<ManagerInner<T>>>,
    token: &str,
) -> Option<Result<T::Meta, &'static str>> {
    let n = rc.borrow().loaders.len();
    for i in 0..n {
        let outcome = {
            let inner = rc.borrow();
            ProbeLoader::<T>::try_probe(&*inner.loaders[i], token)
        };
        match outcome {
            Ok(meta) => return Some(Ok(meta)),
            Err(LoadError::NotMine) => continue,
            Err(LoadError::Failed(msg)) => return Some(Err(msg)),
        }
    }
    None
}
