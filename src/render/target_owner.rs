//! Owner-thread storage for transferable native resource references.

use super::TargetStorage;
use crate::render::RenderError;
use crate::render::budget::Allocation;
use alloc::{boxed::Box, rc::Rc, sync::Arc};
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use core::{any::Any, cell::RefCell, marker::PhantomData};
use std::{sync::OnceLock, thread::ThreadId};

#[cfg(test)]
#[path = "target_owner_tests.rs"]
mod tests;

const ARC_HEADER: usize = 2 * core::mem::size_of::<usize>();

struct Owner {
    thread: ThreadId,
    closed: AtomicBool,
    retired: AtomicBool,
    charge: OnceLock<Allocation>,
}

struct Control {
    owner: Arc<Owner>,
    admitted_bytes: AtomicUsize,
    charge: OnceLock<Allocation>,
}

struct Entry {
    value: Option<Box<dyn Any>>,
    control: Arc<Control>,
    owner: *const Owner,
    storage: Option<TargetStorage>,
    metadata: Option<Allocation>,
    next: Option<Box<Entry>>,
}

impl Entry {
    fn key(&self) -> *const Control {
        Arc::as_ptr(&self.control)
    }

    // Called only after unlinking. Native destruction and ledger release may reenter.
    fn retire(mut self) {
        debug_assert!(self.next.is_none());
        if let Some(metadata) = &mut self.metadata {
            let charge = metadata
                .split(core::mem::size_of::<Control>() + ARC_HEADER)
                .expect("admitted control storage");
            debug_assert!(self.control.charge.get().is_none());
            self.control
                .charge
                .set(charge)
                .expect("single control retirement");
        }
        // The node's strong metadata reference spans worker retirement and raw destruction.
        drop(self.value.take());
        let Self {
            control,
            metadata,
            storage,
            ..
        } = self;
        drop(metadata);
        drop(storage);
        drop(control);
    }
}

#[derive(Default)]
struct Registry {
    head: Option<Box<Entry>>,
}

impl Registry {
    fn get_mut(&mut self, key: *const Control) -> Option<&mut Entry> {
        let mut node = self.head.as_deref_mut();
        while let Some(entry) = node {
            if entry.key() == key {
                return Some(entry);
            }
            node = entry.next.as_deref_mut();
        }
        None
    }

    fn remove(&mut self, mut matches: impl FnMut(&Entry) -> bool) -> Option<Box<Entry>> {
        let mut link = &mut self.head;
        loop {
            if link.as_deref().is_some_and(&mut matches) {
                let mut entry = link.take().expect("matching entry");
                *link = entry.next.take();
                return Some(entry);
            }
            link = &mut link.as_mut()?.next;
        }
    }

    fn extract(&mut self, mut matches: impl FnMut(&Entry) -> bool) -> Option<Box<Entry>> {
        let mut extracted = None;
        let mut link = &mut self.head;
        while let Some(entry) = link.as_deref() {
            if matches(entry) {
                let mut entry = link.take().expect("matching entry");
                *link = entry.next.take();
                entry.next = extracted;
                extracted = Some(entry);
            } else {
                link = &mut link.as_mut().expect("retained entry").next;
            }
        }
        extracted
    }
}

impl Drop for Registry {
    fn drop(&mut self) {
        while let Some(mut entry) = self.head.take() {
            self.head = entry.next.take();
            entry.control.owner.closed.store(true, Ordering::Release);
            (*entry).retire();
        }
    }
}

std::thread_local! {
    static REGISTRY: RefCell<Registry> = const { RefCell::new(Registry { head: None }) };
}

fn registry<R>(
    body: impl FnOnce(&mut Registry) -> Result<R, RenderError>,
) -> Result<R, RenderError> {
    REGISTRY
        .try_with(|registry| {
            let mut registry = registry
                .try_borrow_mut()
                .map_err(|_| RenderError::InvalidScope)?;
            body(&mut registry)
        })
        .map_err(|_| RenderError::InvalidTexture)?
}

fn entry_bytes<T>() -> Option<usize> {
    core::mem::size_of::<Entry>()
        .checked_add(core::mem::size_of::<T>())?
        .checked_add(core::mem::size_of::<Control>() + ARC_HEADER)
}

fn metadata_bytes<T>(owner: &Owner) -> Result<usize, RenderError> {
    let owner_bytes = if owner.charge.get().is_some() {
        0
    } else {
        core::mem::size_of::<Owner>() + ARC_HEADER
    };
    entry_bytes::<T>()
        .and_then(|bytes| bytes.checked_add(owner_bytes))
        .ok_or(RenderError::InvalidGeometry)
}

/// Stores native resources on the creating thread and retires them at explicit safe boundaries.
///
/// Include [`Self::metadata_bytes`] in target admission. Call [`Self::collect`] at idle/frame boundaries and [`Self::close`] before closing the device. This owner is local even when its resource tokens are transferable.
pub struct ThreadOwner {
    owner: Arc<Owner>,
    _local: PhantomData<Rc<()>>,
}

impl Default for ThreadOwner {
    fn default() -> Self {
        Self::new()
    }
}

impl ThreadOwner {
    pub fn new() -> Self {
        // Initialize this TLS slot before registering resources with destructors.
        REGISTRY.with(|_| {});
        Self {
            owner: Arc::new(Owner {
                thread: std::thread::current().id(),
                closed: AtomicBool::new(false),
                retired: AtomicBool::new(false),
                charge: OnceLock::new(),
            }),
            _local: PhantomData,
        }
    }

    /// Additional ownership storage for a target containing `T`, including an uncharged owner.
    ///
    /// This query does not collect retired resources or allocate. Target creation must recheck admission because another target may have retained the owner charge since planning.
    pub fn metadata_bytes<T: 'static>(&self) -> Result<usize, RenderError> {
        self.check()?;
        metadata_bytes::<T>(&self.owner)
    }

    pub fn insert<T: 'static>(&mut self, value: T) -> Result<ThreadResource<T>, RenderError> {
        self.check()?;
        let control = Arc::new(Control {
            owner: self.owner.clone(),
            admitted_bytes: AtomicUsize::new(0),
            charge: OnceLock::new(),
        });
        let mut entry = Some(Box::new(Entry {
            value: Some(Box::new(value)),
            control: control.clone(),
            owner: Arc::as_ptr(&self.owner),
            storage: None,
            metadata: None,
            next: None,
        }));
        registry(|registry| {
            let mut node = entry.take().expect("unregistered entry");
            node.next = registry.head.take();
            registry.head = Some(node);
            Ok(())
        })?;
        Ok(ThreadResource {
            control: Some(control),
            _type: PhantomData,
        })
    }

    /// Destroy retired native resources on this thread without allocating.
    pub fn collect(&mut self) -> Result<(), RenderError> {
        self.check()?;
        if !self.owner.retired.swap(false, Ordering::AcqRel) {
            return Ok(());
        }
        self.drain(false)
    }

    /// Invalidate tokens before destroying idle native resources; active callbacks retire on return.
    pub fn close(&mut self) -> Result<(), RenderError> {
        if std::thread::current().id() != self.owner.thread {
            return Err(RenderError::WrongThread);
        }
        self.owner.closed.store(true, Ordering::Release);
        self.drain(true)
    }

    fn check(&self) -> Result<(), RenderError> {
        if self.owner.closed.load(Ordering::Acquire) {
            Err(RenderError::InvalidTexture)
        } else {
            Ok(())
        }
    }

    fn drain(&self, close: bool) -> Result<(), RenderError> {
        let mut ready = registry(|registry| {
            Ok(registry.extract(|entry| {
                let owned = entry.owner == Arc::as_ptr(&self.owner);
                let retired = Arc::strong_count(&entry.control) == 1;
                owned && entry.value.is_some() && (close || retired)
            }))
        })?;
        while let Some(mut entry) = ready {
            ready = entry.next.take();
            (*entry).retire();
        }
        Ok(())
    }
}

impl Drop for ThreadOwner {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

/// A transferable identity for `T`; native values remain in their creating thread's storage.
pub struct ThreadResource<T: 'static> {
    control: Option<Arc<Control>>,
    _type: PhantomData<fn() -> T>,
}

impl<T: 'static> Clone for ThreadResource<T> {
    fn clone(&self) -> Self {
        Self {
            control: Some(self.control().clone()),
            _type: PhantomData,
        }
    }
}

impl<T: 'static> Drop for ThreadResource<T> {
    fn drop(&mut self) {
        // Publish after decrement: a preceding mark can be consumed before the count changes.
        if let Some(control) = self.control.take() {
            let owner = control.owner.clone();
            let unpublished = control.admitted_bytes.load(Ordering::Acquire) == 0
                && std::thread::current().id() == owner.thread;
            let key = Arc::as_ptr(&control);
            drop(control);
            if unpublished
                && let Ok(Some(entry)) = registry(|registry| {
                    Ok(registry.remove(|entry| {
                        entry.key() == key
                            && Arc::strong_count(&entry.control) == 1
                            && entry.value.is_some()
                    }))
                })
            {
                // Unpublished creation still owns its admission reservation on this thread.
                (*entry).retire();
                return;
            }
            owner.retired.store(true, Ordering::Release);
        }
    }
}

impl<T: 'static> ThreadResource<T> {
    fn control(&self) -> &Arc<Control> {
        self.control.as_ref().expect("live resource token")
    }

    pub fn validate(&self) -> Result<(), RenderError> {
        if self.control().owner.closed.load(Ordering::Acquire) {
            Err(RenderError::InvalidTexture)
        } else {
            Ok(())
        }
    }

    /// Storage charged for this resource, including the owner metadata if admitted here.
    pub fn metadata_bytes(&self) -> Result<usize, RenderError> {
        self.validate()?;
        let admitted = self.control().admitted_bytes.load(Ordering::Acquire);
        if admitted != 0 {
            Ok(admitted)
        } else {
            metadata_bytes::<T>(&self.control().owner)
        }
    }

    /// Partition the existing target charge and retain it with the actual native storage.
    pub fn retain_storage(&self, storage: TargetStorage) -> Result<(), RenderError> {
        self.check_thread()?;
        let mut storage = Some(storage);
        registry(|registry| {
            let entry = registry
                .get_mut(Arc::as_ptr(self.control()))
                .ok_or(RenderError::InvalidTexture)?;
            if entry.storage.is_some() || entry.value.is_none() {
                return Err(RenderError::InvalidScope);
            }
            let bytes = metadata_bytes::<T>(&self.control().owner)?;
            let mut charge = storage
                .as_ref()
                .expect("unattached receipt")
                .split_control(bytes)?;
            if self.control().owner.charge.get().is_none() {
                let owner = charge
                    .split(core::mem::size_of::<Owner>() + ARC_HEADER)
                    .expect("admitted owner storage");
                self.control()
                    .owner
                    .charge
                    .set(owner)
                    .expect("single owner admission");
            }
            entry.metadata = Some(charge);
            entry.storage = storage.take();
            self.control()
                .admitted_bytes
                .store(bytes, Ordering::Release);
            Ok(())
        })
    }

    pub fn with<R>(
        &self,
        body: impl FnOnce(&mut T) -> Result<R, RenderError>,
    ) -> Result<R, RenderError> {
        self.check_thread()?;
        let value = registry(|registry| {
            registry
                .get_mut(Arc::as_ptr(self.control()))
                .ok_or(RenderError::InvalidTexture)?
                .value
                .take()
                .ok_or(RenderError::InvalidScope)
        })?;
        let mut access = Access {
            resource: self,
            value: Some(value),
        };
        body(
            access
                .value
                .as_mut()
                .expect("active resource")
                .downcast_mut::<T>()
                .expect("typed resource identity"),
        )
    }

    fn check_thread(&self) -> Result<(), RenderError> {
        self.validate()?;
        if std::thread::current().id() != self.control().owner.thread {
            Err(RenderError::WrongThread)
        } else {
            Ok(())
        }
    }
}

struct Access<'a, T: 'static> {
    resource: &'a ThreadResource<T>,
    value: Option<Box<dyn Any>>,
}

impl<T: 'static> Drop for Access<'_, T> {
    fn drop(&mut self) {
        let key = Arc::as_ptr(self.resource.control());
        let result = registry(|registry| {
            if self.resource.control().owner.closed.load(Ordering::Acquire) {
                let mut entry = registry
                    .remove(|entry| entry.key() == key)
                    .ok_or(RenderError::InvalidTexture)?;
                entry.value = self.value.take();
                Ok(Some(entry))
            } else {
                let entry = registry.get_mut(key).ok_or(RenderError::InvalidTexture)?;
                entry.value = self.value.take();
                Ok(None)
            }
        });
        if let Ok(Some(entry)) = result {
            (*entry).retire();
        }
    }
}
