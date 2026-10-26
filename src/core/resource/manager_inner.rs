use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::vec::Vec;
use core::any::Any;
use core::cell::{Cell, RefCell};
use core::task::{Poll, Waker};

use hashbrown::{HashMap, HashSet};
use rustc_hash::FxBuildHasher;

use super::invocation::{CallLease, InstalledLoader, Loaded};
use super::{FormatMatch, LoadInput};
use crate::core::cache::{HasSize, LruCache, MaxSize};
use crate::core::reactive::Signal;
use crate::core::resource::loader::LoadError;
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
    loaders: Vec<Rc<dyn InstalledLoader<T>>>,
    fallback: Rc<T>,
    failed_values: HashSet<Cow<'static, str>, FxBuildHasher>,
    signals: HashMap<Cow<'static, str>, Rc<Signal<()>>, FxBuildHasher>,
    refcounts: HashMap<Cow<'static, str>, u32, FxBuildHasher>,
    revision: Rc<Cell<u64>>,
    resolving: HashMap<Cow<'static, str>, Request, FxBuildHasher>,
    next_request: u64,
    wake: Option<Rc<super::wake::TaskWake>>,
}

struct Request {
    generation: u64,
    load: bool,
    queued: bool,
    detached: bool,
}

impl<T: HasSize + 'static> ManagerInner<T> {
    pub(super) fn loader_count(&self) -> usize {
        self.loaders.len()
    }
    pub(super) fn source_request(&self, token: &str) -> Option<(u64, usize)> {
        Some((self.resolving.get(token)?.generation, self.loaders.len()))
    }

    pub(super) fn source_decoder(&self, index: usize) -> Rc<dyn InstalledLoader<T>> {
        self.loaders[index].clone()
    }
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
            wake: None,
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

    pub(crate) fn add_loader(&mut self, loader: Rc<dyn InstalledLoader<T>>) {
        if self.wake.is_none() {
            self.wake = Some(Rc::new(super::wake::TaskWake::new()));
        }
        self.loaders.push(loader);
        self.advance_revision();
    }

    pub(crate) fn revision_cell(&self) -> Rc<Cell<u64>> {
        self.revision.clone()
    }

    pub(super) fn visit_resident(&self, mut visit: impl FnMut(&T)) {
        visit(&self.fallback);
        for (_, value) in self.values.iter() {
            visit(value.get());
        }
    }

    fn advance_revision(&self) {
        self.revision.set(self.revision.get().wrapping_add(1));
    }

    pub(crate) fn bump_refcount(&mut self, token: &str) {
        *self.refcounts.entry(Cow::Owned(token.into())).or_insert(0) += 1;
    }

    pub(crate) fn drop_refcount(&mut self, token: &str) -> bool {
        if let Some(c) = self.refcounts.get_mut(token) {
            *c = c.saturating_sub(1);
            if *c == 0 {
                self.refcounts.remove(token);
                return true;
            }
        }
        false
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

pub(crate) fn release_handle<T: HasSize + 'static>(rc: &Rc<RefCell<ManagerInner<T>>>, token: &str) {
    let retire = {
        let mut inner = rc.borrow_mut();
        if !inner.drop_refcount(token)
            || inner
                .resolving
                .get(token)
                .is_some_and(|request| request.detached)
        {
            false
        } else {
            inner.resolving.remove(token);
            true
        }
    };
    if retire {
        invalidate_calls(rc, token);
    }
}

/// Guards external calls, including unwinding and reentrant replacement.
pub(crate) struct Resolution<'a, T: HasSize + 'static> {
    manager: &'a Rc<RefCell<ManagerInner<T>>>,
    token: &'a str,
    request: u64,
    retained: bool,
}

impl<'a, T: HasSize + 'static> Resolution<'a, T> {
    pub(crate) fn begin(
        manager: &'a Rc<RefCell<ManagerInner<T>>>,
        token: &'a str,
        detached: bool,
    ) -> Option<Self> {
        let mut inner = manager.borrow_mut();
        if let Some(request) = inner.resolving.get_mut(token) {
            request.detached |= detached;
            return None;
        }
        let request = inner
            .next_request
            .checked_add(1)
            .expect("resource request exhausted");
        inner.next_request = request;
        inner.resolving.insert(
            Cow::Owned(token.into()),
            Request {
                generation: request,
                load: false,
                queued: false,
                detached,
            },
        );
        Some(Self {
            manager,
            token,
            request,
            retained: false,
        })
    }

    pub(crate) fn is_current(&self) -> bool {
        self.manager
            .borrow()
            .resolving
            .get(self.token)
            .is_some_and(|request| request.generation == self.request)
    }

    pub(crate) fn retain(&mut self) {
        self.retained = true;
    }
}

impl<T: HasSize + 'static> Drop for Resolution<'_, T> {
    fn drop(&mut self) {
        if !self.retained && self.is_current() {
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
    detached: bool,
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
    let Some(mut resolution) = Resolution::begin(rc, token, detached) else {
        if let Some(request) = rc.borrow_mut().resolving.get_mut(token) {
            request.load = true;
        }
        return ResolveOutcome::Fallback(rc.borrow().fallback_clone());
    };
    rc.borrow_mut().resolving.get_mut(token).unwrap().load = true;

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
    let wake = rc.borrow().wake.clone();
    let parent = wake.as_ref().map_or(Waker::noop(), |wake| wake.waker());
    let chain_outcome = walk_chain(rc, token, resolution.request, false, 0, parent);
    match chain_outcome {
        Poll::Ready(Ok(Loaded::Value(v))) => finish::<T>(rc, token, Some(v), resolution),
        Poll::Ready(_) => finish::<T>(rc, token, None, resolution),
        Poll::Pending => {
            resolution.retain();
            ResolveOutcome::Fallback(rc.borrow().fallback_clone())
        }
    }
}

fn walk_chain<T: HasSize + 'static>(
    rc: &Rc<RefCell<ManagerInner<T>>>,
    token: &str,
    generation: u64,
    probe: bool,
    start: usize,
    parent: &Waker,
) -> Poll<Result<Loaded<T>, LoadError>> {
    let n = rc.borrow().loaders.len();
    for i in start..n {
        let loader = rc.borrow().loaders[i].clone();
        if loader.source_binding().is_some() {
            continue;
        }
        match loader.recognize(LoadInput::Token(token)) {
            FormatMatch::NoMatch => continue,
            FormatMatch::Need(_) => {
                return Poll::Ready(Err(LoadError::Source(super::AssetError::Unsupported)));
            }
            FormatMatch::Match => {}
        }
        if probe && !loader.has_probe() {
            return Poll::Ready(Err(LoadError::NotMine));
        }
        let Some(task) = loader.take(token, generation, probe) else {
            return Poll::Ready(Err(LoadError::Source(super::AssetError::Capacity)));
        };
        let mut lease = CallLease {
            owner: loader,
            task: Some(task),
            retained: false,
        };
        let outcome = lease.task.as_mut().unwrap().poll(parent);
        if !rc
            .borrow()
            .resolving
            .get(token)
            .is_some_and(|request| request.generation == generation)
        {
            return Poll::Ready(Err(LoadError::NotMine));
        }
        match outcome {
            Poll::Pending => {
                lease.retained = true;
                return Poll::Pending;
            }
            Poll::Ready(Err(LoadError::NotMine)) => continue,
            Poll::Ready(Ok(Loaded::Meta(meta))) => {
                lease.task.as_mut().unwrap().call_mut().meta = Some(Ok(meta));
                let meta = lease.task.as_ref().unwrap().clone_meta().unwrap().unwrap();
                lease.retained = true;
                return Poll::Ready(Ok(Loaded::Meta(meta)));
            }
            ready => return ready,
        }
    }
    Poll::Ready(Err(LoadError::NotMine))
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
    let (old, value_rc) = {
        let mut inner = rc.borrow_mut();
        let old = inner.try_acquire_value_clone(token);
        inner.values.drop(token);
        inner.failed_values.remove(token);
        let value = inner.insert_value(Cow::Owned(token.into()), value);
        (old, value)
    };
    drop(old);
    notify(rc, token);
    ResolveOutcome::JustResolved(value_rc)
}

pub(crate) fn walk_probe_chain<T: HasSize + HasProbe + 'static>(
    rc: &Rc<RefCell<ManagerInner<T>>>,
    token: &str,
    resolution: &mut Resolution<'_, T>,
) -> Poll<Option<T::Meta>> {
    let wake = rc.borrow().wake.clone();
    let parent = wake.as_ref().map_or(Waker::noop(), |wake| wake.waker());
    match walk_chain(rc, token, resolution.request, true, 0, parent) {
        Poll::Pending => {
            resolution.retain();
            Poll::Pending
        }
        Poll::Ready(Ok(Loaded::Meta(meta))) => Poll::Ready(Some(
            *meta
                .downcast::<T::Meta>()
                .expect("registered metadata type"),
        )),
        Poll::Ready(_) => Poll::Ready(None),
    }
}

pub(crate) fn completed_probe<T: HasSize + HasProbe + 'static>(
    rc: &Rc<RefCell<ManagerInner<T>>>,
    token: &str,
) -> Option<Result<T::Meta, LoadError>> {
    let n = rc.borrow().loaders.len();
    for i in 0..n {
        let owner = rc.borrow().loaders[i].clone();
        if let Some(task) = owner.take_meta(token) {
            let lease = CallLease {
                owner,
                task: Some(task),
                retained: true,
            };
            return lease.task.as_ref().unwrap().clone_meta().map(|result| {
                result.map(|meta| {
                    *meta
                        .downcast::<T::Meta>()
                        .expect("registered metadata type")
                })
            });
        }
    }
    None
}

pub(crate) fn invalidate_calls<T: HasSize + 'static>(
    rc: &Rc<RefCell<ManagerInner<T>>>,
    token: &str,
) {
    let n = rc.borrow().loaders.len();
    for i in 0..n {
        let loader = rc.borrow().loaders[i].clone();
        loader.invalidate(token);
    }
}

pub(crate) fn reload<T: HasSize + 'static>(
    rc: &Rc<RefCell<ManagerInner<T>>>,
    token: &str,
) -> Result<(), LoadError> {
    let capacity: usize = rc
        .borrow()
        .loaders
        .iter()
        .map(|loader| loader.capacity())
        .sum();
    if capacity == 0 {
        return Err(LoadError::NotMine);
    }
    let wake = {
        let mut inner = rc.borrow_mut();
        if !inner.resolving.contains_key(token) && inner.resolving.len() >= capacity {
            return Err(super::AssetError::Capacity.into());
        }
        let generation = inner
            .next_request
            .checked_add(1)
            .ok_or(super::AssetError::Capacity)?;
        inner.next_request = generation;
        inner.resolving.insert(
            Cow::Owned(token.into()),
            Request {
                generation,
                load: true,
                queued: true,
                detached: true,
            },
        );
        inner.failed_values.remove(token);
        inner.wake.as_ref().unwrap().clone()
    };
    invalidate_calls(rc, token);
    wake.schedule();
    Ok(())
}

/// Poll each ready slot at most once. Waking during a poll schedules a later turn.
pub(crate) fn poll_pending<T: HasSize + 'static>(
    rc: &Rc<RefCell<ManagerInner<T>>>,
    parent: &Waker,
) {
    let Some(wake) = rc.borrow().wake.clone() else {
        return;
    };
    let ready = wake.take_ready();
    wake.register(parent);
    if !ready {
        return;
    }
    let parent = wake.waker();
    let queued_through = rc.borrow().next_request;
    let n = rc.borrow().loaders.len();
    for i in 0..n {
        let owner = rc.borrow().loaders[i].clone();
        for slot in 0..owner.capacity() {
            let Some(task) = owner.take_ready(slot, parent) else {
                continue;
            };
            let mut lease = CallLease {
                owner: owner.clone(),
                task: Some(task),
                retained: false,
            };
            let call = lease.task.as_ref().unwrap().call();
            let generation = call.generation;
            let current = rc
                .borrow()
                .resolving
                .get(call.token.as_deref().unwrap())
                .is_some_and(|request| request.generation == generation);
            if !current {
                continue;
            }
            let mut guard = PendingGuard {
                manager: rc,
                generation,
                retained: false,
            };
            let outcome = lease.task.as_mut().unwrap().poll(parent);
            let call = lease.task.as_ref().unwrap().call();
            if !rc
                .borrow()
                .resolving
                .get(call.token.as_deref().unwrap())
                .is_some_and(|request| request.generation == generation)
            {
                continue;
            }
            if outcome.is_pending() {
                lease.retained = true;
                guard.retained = true;
                continue;
            }
            let call = lease.task.as_ref().unwrap().call();
            let token = call.token.as_ref().unwrap().clone();
            let wanted = rc
                .borrow()
                .resolving
                .get(token.as_ref())
                .filter(|request| request.generation == generation)
                .map(|request| request.load);
            let Some(wanted) = wanted else {
                continue;
            };
            let outcome = match outcome {
                Poll::Ready(Err(LoadError::NotMine)) => {
                    let probe = call.probe && !wanted;
                    let next = walk_chain(rc, &token, generation, probe, i + 1, parent);
                    if let Poll::Ready(Err(error)) = &next {
                        if probe {
                            lease.task.as_mut().unwrap().call_mut().meta = Some(Err(*error));
                            lease.retained = true;
                            drop(lease);
                            drop(guard);
                            rc.borrow().advance_revision();
                            notify(rc, &token);
                            continue;
                        }
                    }
                    drop(lease);
                    next
                }
                Poll::Ready(Ok(Loaded::Meta(meta))) => {
                    let call = lease.task.as_mut().unwrap().call_mut();
                    call.meta = Some(Ok(meta));
                    if wanted {
                        call.probe = false;
                        call.meta = None;
                        call.wake.schedule();
                        lease.retained = true;
                        guard.retained = true;
                        continue;
                    }
                    lease.retained = true;
                    drop(lease);
                    drop(guard);
                    rc.borrow().advance_revision();
                    notify(rc, &token);
                    continue;
                }
                Poll::Ready(Err(error)) if call.probe && !wanted => {
                    lease.task.as_mut().unwrap().call_mut().meta = Some(Err(error));
                    lease.retained = true;
                    drop(lease);
                    drop(guard);
                    rc.borrow().advance_revision();
                    notify(rc, &token);
                    continue;
                }
                result => {
                    drop(lease);
                    result
                }
            };
            if !rc
                .borrow()
                .resolving
                .get(token.as_ref())
                .is_some_and(|request| request.generation == generation)
            {
                continue;
            }
            match outcome {
                Poll::Pending => guard.retained = true,
                Poll::Ready(Ok(Loaded::Value(value))) => {
                    drop(guard);
                    publish_value(rc, &token, value);
                }
                Poll::Ready(Ok(Loaded::Meta(_))) => {
                    drop(guard);
                    rc.borrow().advance_revision();
                    notify(rc, &token);
                }
                Poll::Ready(Err(_)) => {
                    if wanted {
                        rc.borrow_mut().mark_failed(&token);
                    }
                    drop(guard);
                    rc.borrow().advance_revision();
                    notify(rc, &token);
                }
            }
        }
    }
    loop {
        let queued = {
            let mut inner = rc.borrow_mut();
            inner.resolving.iter_mut().find_map(|(token, request)| {
                if request.queued && request.generation <= queued_through {
                    request.queued = false;
                    Some((token.clone(), request.generation))
                } else {
                    None
                }
            })
        };
        let Some((token, generation)) = queued else {
            break;
        };
        let mut resolution = Resolution {
            manager: rc,
            token: &token,
            request: generation,
            retained: false,
        };
        match walk_chain(rc, &token, generation, false, 0, parent) {
            Poll::Pending => resolution.retain(),
            Poll::Ready(Ok(Loaded::Value(value))) => {
                finish(rc, &token, Some(value), resolution);
            }
            Poll::Ready(_) => {
                finish(rc, &token, None, resolution);
            }
        }
    }
}

struct PendingGuard<'a, T: HasSize + 'static> {
    manager: &'a Rc<RefCell<ManagerInner<T>>>,
    generation: u64,
    retained: bool,
}

impl<T: HasSize + 'static> Drop for PendingGuard<'_, T> {
    fn drop(&mut self) {
        if !self.retained {
            self.manager
                .borrow_mut()
                .resolving
                .retain(|_, request| request.generation != self.generation);
        }
    }
}
