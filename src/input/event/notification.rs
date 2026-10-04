use alloc::vec::Vec;
use core::any::TypeId;
use core::cell::{Cell, RefCell};
use core::mem::{self, size_of};
use core::ptr::NonNull;

use crate::ecs::Entity;
use crate::ecs::payload::{PayloadArena, PayloadError, drop_stored};

use super::contract::{AppNotification, EntityNotification, Event};

/// Logical capacity for each half of notification storage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NotificationCapacity {
    pub records: usize,
    pub payload_bytes: usize,
    pub payload_alignment: usize,
}

impl Default for NotificationCapacity {
    fn default() -> Self {
        Self {
            records: 0,
            payload_bytes: 0,
            payload_alignment: 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotificationError {
    RecordCapacity { available: usize },
    PayloadCapacity { required: usize, available: usize },
    PayloadAlignment { required: usize, available: usize },
    SizeOverflow,
    InvalidLayout,
    AllocationFailed,
    Busy,
    FlushLimit { limit: usize },
}

impl From<PayloadError> for NotificationError {
    fn from(error: PayloadError) -> Self {
        match error {
            PayloadError::SizeOverflow => Self::SizeOverflow,
            PayloadError::InvalidLayout => Self::InvalidLayout,
            PayloadError::AllocationFailed => Self::AllocationFailed,
            PayloadError::Capacity {
                required,
                available,
            } => Self::PayloadCapacity {
                required,
                available,
            },
            PayloadError::Alignment {
                required,
                available,
            } => Self::PayloadAlignment {
                required,
                available,
            },
        }
    }
}

#[derive(Clone, Copy)]
struct NotificationRecord {
    kind: TypeId,
    entity: Option<Entity>,
    offset: usize,
    drop_payload: unsafe fn(*mut u8),
}

#[derive(Default)]
struct NotificationBuffer {
    records: Vec<NotificationRecord>,
    payload: PayloadArena,
    cursor: usize,
}

impl NotificationBuffer {
    fn reserve(&mut self, capacity: NotificationCapacity) -> Result<(), NotificationError> {
        self.payload
            .reserve(capacity.payload_bytes, capacity.payload_alignment)?;
        self.records
            .try_reserve_exact(capacity.records.saturating_sub(self.records.len()))
            .map_err(|_| NotificationError::AllocationFailed)?;
        Ok(())
    }

    fn push<E: Event>(
        &mut self,
        entity: Option<Entity>,
        value: E,
        capacity: NotificationCapacity,
    ) -> Result<(), (NotificationError, E)> {
        if self.records.len() >= capacity.records {
            return Err((
                NotificationError::RecordCapacity {
                    available: capacity.records,
                },
                value,
            ));
        }
        let (required, alignment) = match self.payload.required::<E>() {
            Ok(required) => required,
            Err(error) => return Err((error.into(), value)),
        };
        if alignment > capacity.payload_alignment {
            return Err((
                NotificationError::PayloadAlignment {
                    required: alignment,
                    available: capacity.payload_alignment,
                },
                value,
            ));
        }
        if required > capacity.payload_bytes {
            return Err((
                NotificationError::PayloadCapacity {
                    required,
                    available: capacity.payload_bytes,
                },
                value,
            ));
        }
        let offset = match self.payload.try_store(value) {
            Ok(offset) => offset,
            Err((error, value)) => return Err((error.into(), value)),
        };
        self.records.push(NotificationRecord {
            kind: TypeId::of::<E>(),
            entity,
            offset,
            drop_payload: drop_stored::<E>,
        });
        Ok(())
    }

    fn is_empty(&self) -> bool {
        self.cursor == self.records.len()
    }

    fn next(&mut self) -> Option<Notification<'_>> {
        let record = *self.records.get(self.cursor)?;
        self.cursor += 1;
        Some(Notification {
            record,
            payload: self.payload.ptr(record.offset),
            storage: core::marker::PhantomData,
        })
    }

    fn clear(&mut self) {
        while let Some(notification) = self.next() {
            drop(notification);
        }
        self.records.clear();
        self.payload.clear();
        self.cursor = 0;
    }
}

impl Drop for NotificationBuffer {
    fn drop(&mut self) {
        self.clear();
    }
}

/// A stable, readonly payload window during one notification invocation.
pub struct Notification<'storage> {
    record: NotificationRecord,
    payload: *mut u8,
    storage: core::marker::PhantomData<&'storage mut PayloadArena>,
}

impl Notification<'_> {
    pub fn entity(&self) -> Option<Entity> {
        self.record.entity
    }

    pub fn payload<E: Event>(&self) -> Option<&E> {
        if self.record.kind != TypeId::of::<E>() {
            return None;
        }
        let pointer = if size_of::<E>() == 0 {
            NonNull::<E>::dangling().as_ptr()
        } else {
            self.payload.cast::<E>()
        };
        // SAFETY: the record's TypeId matches the stored E. Its buffer cannot
        // move or be cleared while this window is borrowed.
        Some(unsafe { &*pointer })
    }
}

impl Drop for Notification<'_> {
    fn drop(&mut self) {
        // SAFETY: next transferred ownership of exactly this unread record.
        unsafe { (self.record.drop_payload)(self.payload) };
    }
}

#[derive(Default)]
struct QueueState {
    read: NotificationBuffer,
    write: NotificationBuffer,
    capacity: NotificationCapacity,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum QueuePhase {
    #[default]
    Idle,
    Flushing,
    Discarding,
}

/// Storage for bounded event notification batches, not an event router.
#[derive(Default)]
pub struct NotificationQueue {
    state: RefCell<QueueState>,
    phase: Cell<QueuePhase>,
}

impl NotificationQueue {
    /// Reserve both buffers. Logical limits change only after both succeed.
    pub fn reserve(&self, requested: NotificationCapacity) -> Result<(), NotificationError> {
        if self.phase.get() != QueuePhase::Idle {
            return Err(NotificationError::Busy);
        }
        core::alloc::Layout::from_size_align(requested.payload_bytes, requested.payload_alignment)
            .map_err(|_| NotificationError::InvalidLayout)?;
        let mut state = self.state.borrow_mut();
        let capacity = NotificationCapacity {
            records: requested.records.max(state.capacity.records),
            payload_bytes: requested.payload_bytes.max(state.capacity.payload_bytes),
            payload_alignment: requested
                .payload_alignment
                .max(state.capacity.payload_alignment),
        };
        core::alloc::Layout::array::<NotificationRecord>(capacity.records)
            .map_err(|_| NotificationError::InvalidLayout)?;
        state.read.reserve(capacity)?;
        state.write.reserve(capacity)?;
        state.capacity = capacity;
        Ok(())
    }

    pub fn push_entity<E: Event<Route = EntityNotification>>(
        &self,
        entity: Entity,
        value: E,
    ) -> Result<(), (NotificationError, E)> {
        self.push(Some(entity), value)
    }

    pub fn push_app<E: Event<Route = AppNotification>>(
        &self,
        value: E,
    ) -> Result<(), (NotificationError, E)> {
        self.push(None, value)
    }

    fn push<E: Event>(
        &self,
        entity: Option<Entity>,
        value: E,
    ) -> Result<(), (NotificationError, E)> {
        if self.phase.get() == QueuePhase::Discarding {
            return Err((NotificationError::Busy, value));
        }
        let mut state = self.state.borrow_mut();
        let capacity = state.capacity;
        state.write.push(entity, value, capacity)
    }

    pub fn clear(&self) -> Result<(), NotificationError> {
        if self.phase.get() != QueuePhase::Idle {
            return Err(NotificationError::Busy);
        }
        self.phase.set(QueuePhase::Discarding);
        let _phase = PhaseReset(&self.phase);
        let mut state = self.state.borrow_mut();
        state.read.clear();
        state.write.clear();
        Ok(())
    }

    /// Drain in FIFO order; callback writes wait behind all older records.
    /// An interrupted flush drops both the remaining batch and callback writes.
    pub fn flush(
        &self,
        limit: usize,
        mut run: impl FnMut(&Notification<'_>),
    ) -> Result<usize, NotificationError> {
        if self.phase.get() != QueuePhase::Idle {
            return Err(NotificationError::Busy);
        }
        self.phase.set(QueuePhase::Flushing);
        let read = mem::take(&mut self.state.borrow_mut().read);
        let mut guard = FlushGuard { queue: self, read };
        let mut dispatched = 0;
        loop {
            if guard.read.is_empty() {
                guard.read.clear();
                let mut state = self.state.borrow_mut();
                if state.write.is_empty() {
                    return Ok(dispatched);
                }
                mem::swap(&mut guard.read, &mut state.write);
            }
            if dispatched == limit {
                return Err(NotificationError::FlushLimit { limit });
            }
            let notification = guard.read.next().expect("non-empty notification batch");
            run(&notification);
            dispatched += 1;
        }
    }
}

struct FlushGuard<'queue> {
    queue: &'queue NotificationQueue,
    read: NotificationBuffer,
}

impl Drop for FlushGuard<'_> {
    fn drop(&mut self) {
        self.queue.phase.set(QueuePhase::Discarding);
        let _phase = PhaseReset(&self.queue.phase);
        self.read.clear();
        let mut state = self.queue.state.borrow_mut();
        state.write.clear();
        mem::swap(&mut state.read, &mut self.read);
    }
}

struct PhaseReset<'queue>(&'queue Cell<QueuePhase>);

impl Drop for PhaseReset<'_> {
    fn drop(&mut self) {
        self.0.set(QueuePhase::Idle);
    }
}

#[cfg(test)]
mod tests;
