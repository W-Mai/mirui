use crate::core::cache::Handle;
use alloc::borrow::Cow;
use alloc::rc::{Rc, Weak};
use core::cell::{Cell, RefCell};

use crate::core::cache::HasSize;
use crate::core::resource::manager_inner::{self, ManagerInner, ResolveOutcome};
use crate::core::resource::probe::HasProbe;

/// Lease on a token registered with a [`ResourceManager`](super::ResourceManager). Cloning and dropping update the manager's token demand. [`Self::get`] returns a shared value handle that retains its contents independently of the manager; after the manager is dropped, subsequent calls return the retained fallback.
pub struct ResourceHandle<T: HasSize + 'static> {
    token: Cow<'static, str>,
    manager: Weak<RefCell<ManagerInner<T>>>,
    /// Held outside the manager so `get()` can answer with a stable value
    /// after `Weak::upgrade` returns `None` (manager has been dropped).
    fallback: Handle<T>,
    revision: Rc<Cell<u64>>,
    cached: RefCell<Option<(u64, Handle<T>)>>,
}

impl<T: HasSize + 'static> ResourceHandle<T> {
    pub(super) fn new(
        token: Cow<'static, str>,
        manager: Weak<RefCell<ManagerInner<T>>>,
        fallback: Handle<T>,
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

    pub(crate) fn signal(&self) -> Option<Rc<crate::core::reactive::Signal<()>>> {
        let manager = self.manager.upgrade()?;
        Some(manager.borrow_mut().signal_for(self.token.clone()))
    }

    pub(crate) fn get_cached(&self) -> Option<Handle<T>> {
        let revision = self.revision.get();
        self.cached
            .borrow()
            .as_ref()
            .filter(|(cached_revision, _)| *cached_revision == revision)
            .map(|(_, value)| value.clone())
    }

    pub fn get(&self) -> Handle<T> {
        let Some(rc) = self.manager.upgrade() else {
            return self.fallback.clone();
        };
        let revision = self.revision.get();
        if let Some(value) = self.get_cached() {
            return value;
        }
        let (revision, value) = match manager_inner::resolve(&rc, &self.token, false) {
            ResolveOutcome::CacheHit(v) | ResolveOutcome::JustResolved(v) => {
                (self.revision.get(), v)
            }
            ResolveOutcome::Fallback(v) => (revision, v),
        };
        *self.cached.borrow_mut() = Some((revision, value.clone()));
        value
    }
}

impl<T: HasSize + HasProbe + 'static> ResourceHandle<T> {
    pub fn probe_via_manager(
        &self,
        manager: &crate::core::resource::manager::ResourceManager<T>,
    ) -> Option<T::Meta> {
        manager.probe_with_demand(&self.token, false)
    }
}

impl<T: HasSize + 'static> Clone for ResourceHandle<T> {
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

impl<T: HasSize + 'static> Drop for ResourceHandle<T> {
    fn drop(&mut self) {
        if let Some(rc) = self.manager.upgrade() {
            manager_inner::release_handle(&rc, &self.token);
        }
    }
}
