use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::vec::Vec;
use core::any::Any;
use core::cell::{Cell, RefCell};

use hashbrown::{HashMap, HashSet};
use rustc_hash::FxBuildHasher;

use crate::core::cache::{HasSize, LruCache, MaxSize};
use crate::core::reactive::Signal;
use crate::core::resource::loader::{LoadError, Loader, ProbeLoader};
use crate::core::resource::probe::HasProbe;

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

pub struct ManagerInner<T: HasSize + 'static> {
    values: LruCache<Cow<'static, str>, Rc<T>>,
    by_token: HashMap<Cow<'static, str>, Entry<T>, FxBuildHasher>,
    loaders: Vec<LoaderEntry<T>>,
    fallback: Rc<T>,
    failed_values: HashSet<Cow<'static, str>, FxBuildHasher>,
    signals: HashMap<Cow<'static, str>, Rc<Signal<()>>, FxBuildHasher>,
    refcounts: HashMap<Cow<'static, str>, u32, FxBuildHasher>,
    revision: Rc<Cell<u64>>,
    resolving: HashMap<Cow<'static, str>, u64, FxBuildHasher>,
    next_request: u64,
}

type ProbeCallback = dyn Fn(&str) -> Result<Box<dyn Any>, LoadError>;

struct LoaderEntry<T> {
    loader: Rc<dyn Loader<T>>,
    probe: Option<Rc<ProbeCallback>>,
}

impl<T: HasSize + 'static> ManagerInner<T> {
    pub(crate) fn new(values_budget: MaxSize, fallback: T) -> Self {
        Self {
            values: LruCache::new(values_budget),
            by_token: HashMap::default(),
            loaders: Vec::new(),
            fallback: Rc::new(fallback),
            failed_values: HashSet::default(),
            signals: HashMap::default(),
            refcounts: HashMap::default(),
            revision: Rc::new(Cell::new(0)),
            resolving: HashMap::default(),
            next_request: 0,
        }
    }

    // ---- fallback / loaders / refcount / signals ----

    pub(crate) fn set_fallback(&mut self, value: T) {
        self.fallback = Rc::new(value);
        self.advance_revision();
    }

    pub(crate) fn fallback_clone(&self) -> Rc<T> {
        self.fallback.clone()
    }

    pub(crate) fn add_loader(&mut self, loader: impl Loader<T>) {
        self.loaders.push(LoaderEntry {
            loader: Rc::new(loader),
            probe: None,
        });
        self.advance_revision();
    }

    pub(crate) fn add_probed_loader(&mut self, loader: impl ProbeLoader<T>)
    where
        T: HasProbe,
    {
        let loader = Rc::new(loader);
        let probe = loader.clone();
        self.loaders.push(LoaderEntry {
            loader,
            probe: Some(Rc::new(move |token| {
                probe
                    .try_probe(token)
                    .map(|meta| Box::new(meta) as Box<dyn Any>)
            })),
        });
        self.advance_revision();
    }

    pub(crate) fn revision_cell(&self) -> Rc<Cell<u64>> {
        self.revision.clone()
    }

    fn advance_revision(&self) {
        self.revision.set(self.revision.get().wrapping_add(1));
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

    pub(crate) fn signal(&self, token: &str) -> Option<Rc<Signal<()>>> {
        self.signals.get(token).cloned()
    }

    // ---- entry registration / removal ----

    pub(crate) fn insert_entry(&mut self, token: Cow<'static, str>, entry: Entry<T>) {
        self.resolving.remove(token.as_ref());
        self.failed_values.remove(token.as_ref());
        self.values.drop(token.as_ref());
        self.by_token.insert(token, entry);
        self.advance_revision();
    }

    pub(crate) fn take_entry(&mut self, token: &str) -> Option<Entry<T>> {
        self.by_token.remove(token)
    }

    pub(crate) fn restore_entry(&mut self, token: Cow<'static, str>, entry: Entry<T>) {
        self.by_token.insert(token, entry);
    }

    pub(crate) fn peek_entry_kind(&self, token: &str) -> Option<EntryKind> {
        self.by_token.get(token).map(Entry::kind)
    }

    pub(crate) fn unregister(&mut self, token: &str) {
        self.resolving.remove(token);
        self.by_token.remove(token);
        self.failed_values.remove(token);
        self.values.drop(token);
        if self
            .signals
            .get(token)
            .is_some_and(|signal| Rc::strong_count(signal) == 1 && !signal.has_subscribers())
        {
            self.signals.remove(token);
        }
        self.advance_revision();
    }

    // ---- failed-set ----

    pub(crate) fn mark_failed(&mut self, token: &str) {
        self.failed_values.insert(Cow::Owned(token.into()));
    }

    pub(crate) fn clear_failed(&mut self, token: &str) {
        if self.failed_values.remove(token) {
            self.advance_revision();
        }
    }

    pub(crate) fn clear_all_failed(&mut self) {
        if !self.failed_values.is_empty() {
            self.failed_values.clear();
            self.advance_revision();
        }
    }

    pub(crate) fn is_failed(&self, token: &str) -> bool {
        self.failed_values.contains(token)
    }

    // ---- value cache helpers ----

    pub(crate) fn try_acquire_value_clone(&mut self, token: &str) -> Option<Rc<T>> {
        let h = crate::trace_span!("res.cache_acquire", { self.values.acquire(token)? });
        let rc = crate::trace_span!("res.rc_clone", { (*h).clone() });
        Some(rc)
    }

    pub(crate) fn insert_value(&mut self, token: Cow<'static, str>, value: T) -> Rc<T> {
        let rc = Rc::new(value);
        let handle = self
            .values
            .entry(token.clone())
            .or_insert_with(|| rc.clone());
        let result = (*handle).clone();
        self.advance_revision();
        result
    }
}

pub(crate) fn notify<T: HasSize + 'static>(rc: &Rc<RefCell<ManagerInner<T>>>, token: &str) {
    let signal = rc.borrow().signal(token);
    if let Some(signal) = signal {
        signal.set(());
    }
}

/// Guards external calls, including unwinding and reentrant replacement.
pub(crate) struct Resolution<'a, T: HasSize + 'static> {
    manager: &'a Rc<RefCell<ManagerInner<T>>>,
    token: &'a str,
    request: u64,
}

impl<'a, T: HasSize + 'static> Resolution<'a, T> {
    pub(crate) fn begin(manager: &'a Rc<RefCell<ManagerInner<T>>>, token: &'a str) -> Option<Self> {
        let mut inner = manager.borrow_mut();
        if inner.resolving.contains_key(token) {
            return None;
        }
        let request = inner
            .next_request
            .checked_add(1)
            .expect("resource request exhausted");
        inner.next_request = request;
        inner.resolving.insert(Cow::Owned(token.into()), request);
        Some(Self {
            manager,
            token,
            request,
        })
    }

    pub(crate) fn is_current(&self) -> bool {
        self.manager.borrow().resolving.get(self.token) == Some(&self.request)
    }
}

impl<T: HasSize + 'static> Drop for Resolution<'_, T> {
    fn drop(&mut self) {
        if self.is_current() {
            self.manager.borrow_mut().resolving.remove(self.token);
        }
    }
}

pub(crate) enum ResolveOutcome<T> {
    CacheHit(Rc<T>),
    JustResolved(Rc<T>),
    Fallback(Rc<T>),
}

pub(crate) fn resolve<T: HasSize + 'static>(
    rc: &Rc<RefCell<ManagerInner<T>>>,
    token: &str,
) -> ResolveOutcome<T> {
    // Step 1: values cache hit
    {
        let mut inner = crate::trace_span!("res.borrow_mut", { rc.borrow_mut() });
        let acquired =
            crate::trace_span!("res.try_acquire", { inner.try_acquire_value_clone(token) });
        if let Some(v) = acquired {
            return ResolveOutcome::CacheHit(v);
        }
        // Step 2: known-failed
        if inner.is_failed(token) {
            return ResolveOutcome::Fallback(inner.fallback_clone());
        }
    }

    let entry_taken = rc.borrow_mut().take_entry(token);
    // Static values have no loader callback or pending task to guard.
    if let Some(Entry::Static(value)) = entry_taken {
        return publish_value(rc, token, value);
    }
    let Some(resolution) = Resolution::begin(rc, token) else {
        return ResolveOutcome::Fallback(rc.borrow().fallback_clone());
    };

    // Step 3: by_token
    if let Some(entry) = entry_taken {
        let value = match entry {
            Entry::Static(v) => Some(v),
            Entry::Factory(f) | Entry::FactoryWithProbe { factory: f, .. } => f(),
        };
        return finish::<T>(rc, token, value, resolution);
    }

    // Step 4: loader chain. Walk through loaders and, on the first claim, pull
    // the value out without holding a borrow across the loader call (so loaders
    // can call back into the manager without triggering a re-entrant
    // borrow_mut panic).
    let chain_outcome = walk_load_chain(rc, token);
    match chain_outcome {
        ChainOutcome::Loaded(v) => finish::<T>(rc, token, Some(v), resolution),
        ChainOutcome::Failed(_) | ChainOutcome::AllNotMine => {
            finish::<T>(rc, token, None, resolution)
        }
    }
}

enum ChainOutcome<T> {
    Loaded(T),
    #[allow(dead_code)] // message is surfaced at the LoadError::Failed boundary
    Failed(&'static str),
    AllNotMine,
}

fn walk_load_chain<T: HasSize + 'static>(
    rc: &Rc<RefCell<ManagerInner<T>>>,
    token: &str,
) -> ChainOutcome<T> {
    let n = rc.borrow().loaders.len();
    for i in 0..n {
        let loader = rc.borrow().loaders[i].loader.clone();
        let outcome = loader.try_load(token);
        match outcome {
            Ok(v) => return ChainOutcome::Loaded(v),
            Err(LoadError::NotMine) => continue,
            Err(LoadError::Failed(msg)) => return ChainOutcome::Failed(msg),
        }
    }
    ChainOutcome::AllNotMine
}

fn finish<T: HasSize + 'static>(
    rc: &Rc<RefCell<ManagerInner<T>>>,
    token: &str,
    value: Option<T>,
    resolution: Resolution<'_, T>,
) -> ResolveOutcome<T> {
    if !resolution.is_current() {
        let mut inner = rc.borrow_mut();
        let value = inner
            .try_acquire_value_clone(token)
            .unwrap_or_else(|| inner.fallback_clone());
        return ResolveOutcome::Fallback(value);
    }
    let Some(v) = value else {
        let mut inner = rc.borrow_mut();
        inner.mark_failed(token);
        return ResolveOutcome::Fallback(inner.fallback_clone());
    };

    drop(resolution);
    publish_value(rc, token, v)
}

fn publish_value<T: HasSize + 'static>(
    rc: &Rc<RefCell<ManagerInner<T>>>,
    token: &str,
    value: T,
) -> ResolveOutcome<T> {
    let value_rc = rc
        .borrow_mut()
        .insert_value(Cow::Owned(token.into()), value);
    notify(rc, token);
    ResolveOutcome::JustResolved(value_rc)
}

/// Walk the loader chain calling [`ProbeLoader::try_probe`] on each. Lives
/// here next to [`ManagerInner`] so it can iterate `loaders` without exposing
/// the field, and requires `T: HasProbe` so it only exists for probe-capable
/// resources.
pub(crate) fn walk_probe_chain<T: HasSize + HasProbe + 'static>(
    rc: &Rc<RefCell<ManagerInner<T>>>,
    token: &str,
) -> Option<Result<T::Meta, &'static str>> {
    let n = rc.borrow().loaders.len();
    for i in 0..n {
        // An unprobed loader may own this token. Do not borrow metadata from a later loader that might produce a different value.
        let probe = rc.borrow().loaders[i].probe.clone()?;
        let outcome = probe(token);
        match outcome {
            Ok(meta) => {
                return Some(Ok(*meta
                    .downcast::<T::Meta>()
                    .expect("typed probe registration")));
            }
            Err(LoadError::NotMine) => continue,
            Err(LoadError::Failed(msg)) => return Some(Err(msg)),
        }
    }
    None
}
