use alloc::borrow::Cow;
use alloc::rc::{Rc, Weak};
use core::cell::{Cell, RefCell};

use crate::core::cache::HasSize;
use crate::core::resource::manager_inner::{self, ManagerInner, ResolveOutcome};
use crate::core::resource::probe::HasProbe;

/// Lease on a token registered with a [`ResourceManager`]. Holding a handle
/// bumps the manager's per-token refcount; cloning bumps it again, dropping
/// decrements it. Use [`ResourceHandle::get`] for programmatic access — it
/// returns an [`Rc<T>`] that survives the manager being dropped (handle
/// keeps a fallback ref so `get` is always answerable).
pub struct ResourceHandle<T: HasSize + Clone + 'static> {
    token: Cow<'static, str>,
    manager: Weak<RefCell<ManagerInner<T>>>,
    /// Held outside the manager so `get()` can answer with a stable value
    /// after `Weak::upgrade` returns `None` (manager has been dropped).
    fallback: Rc<T>,
    revision: Rc<Cell<u64>>,
    cached: RefCell<Option<(u64, Rc<T>)>>,
}

impl<T: HasSize + Clone + 'static> ResourceHandle<T> {
    pub(super) fn new(
        token: Cow<'static, str>,
        manager: Weak<RefCell<ManagerInner<T>>>,
        fallback: Rc<T>,
        revision: Rc<Cell<u64>>,
    ) -> Self {
        Self {
            token,
            manager,
            fallback,
            revision,
            cached: RefCell::new(None),
        }
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    pub(crate) fn get_cached(&self) -> Option<Rc<T>> {
        let revision = self.revision.get();
        self.cached
            .borrow()
            .as_ref()
            .filter(|(cached_revision, _)| *cached_revision == revision)
            .map(|(_, value)| value.clone())
    }

    pub fn get(&self) -> Rc<T> {
        let Some(rc) = self.manager.upgrade() else {
            return self.fallback.clone();
        };
        let revision = self.revision.get();
        if let Some(value) = self.get_cached() {
            return value;
        }
        let value = match manager_inner::resolve(&rc, &self.token) {
            ResolveOutcome::CacheHit(v) => v,
            ResolveOutcome::JustResolved { value, .. } => value,
            ResolveOutcome::Fallback(v) => v,
        };
        *self.cached.borrow_mut() = Some((revision, value.clone()));
        value
    }
}

impl<T: HasSize + HasProbe + Clone + 'static> ResourceHandle<T> {
    pub fn probe_via_manager(
        &self,
        manager: &crate::core::resource::manager::ResourceManager<T>,
    ) -> Option<T::Meta> {
        manager.probe(&self.token)
    }
}

impl<T: HasSize + Clone + 'static> Clone for ResourceHandle<T> {
    fn clone(&self) -> Self {
        if let Some(rc) = self.manager.upgrade() {
            rc.borrow_mut().bump_refcount(&self.token);
        }
        Self {
            token: self.token.clone(),
            manager: self.manager.clone(),
            fallback: self.fallback.clone(),
            revision: self.revision.clone(),
            cached: RefCell::new(self.cached.borrow().clone()),
        }
    }
}

impl<T: HasSize + Clone + 'static> Drop for ResourceHandle<T> {
    fn drop(&mut self) {
        if let Some(rc) = self.manager.upgrade() {
            rc.borrow_mut().drop_refcount(&self.token);
        }
    }
}
