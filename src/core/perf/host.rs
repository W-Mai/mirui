use super::{BatchInfo, CaptureError, PerfEvent, TraceStatus, buffer::Buffer};
use core::sync::atomic::{AtomicBool, Ordering};
use std::cell::RefCell;
use std::sync::{Arc, Mutex, Weak};
use std::vec::Vec;

static ENABLED: AtomicBool = AtomicBool::new(false);
static REGISTRY: Mutex<Registry> = Mutex::new(Registry {
    epoch: 0,
    owner: false,
    next_track: 0,
    capacity: 0,
    unregistered: 0,
    tracks: Vec::new(),
    next_scope: 0,
});

struct Registry {
    epoch: u64,
    owner: bool,
    next_track: u64,
    capacity: usize,
    unregistered: u64,
    tracks: Vec<Arc<Track>>,
    next_scope: u64,
}

struct Track {
    epoch: u64,
    id: u64,
    buffer: Mutex<Buffer>,
}

struct Registration(Arc<Track>);
impl Drop for Registration {
    fn drop(&mut self) {
        self.0.buffer.lock().unwrap().closed = true;
    }
}

thread_local! {
    static TRACK: RefCell<Option<Registration>> = const { RefCell::new(None) };
    static OPERATION: core::cell::Cell<Option<super::SubmissionId>> = const { core::cell::Cell::new(None) };
}

// Includes registry/TLS slots and a conservative allowance for Arc control data.
pub(super) const OVERHEAD: usize = core::mem::size_of::<Registry>()
    + core::mem::size_of::<Track>()
    + core::mem::size_of::<Registration>()
    + 64;

pub(super) fn start(capacity: usize) -> Result<u64, CaptureError> {
    let mut registry = REGISTRY.lock().unwrap();
    if registry.owner {
        return Err(CaptureError::InUse);
    }
    registry.epoch = registry
        .epoch
        .checked_add(1)
        .ok_or(CaptureError::Exhausted)?;
    registry.owner = true;
    registry.capacity = capacity;
    registry.next_track = 0;
    registry.next_scope = 0;
    registry.unregistered = 0;
    let epoch = registry.epoch;
    ENABLED.store(true, Ordering::Relaxed);
    drop(registry);
    if let Err(error) = register_thread() {
        release(epoch);
        return Err(error);
    }
    Ok(epoch)
}

/// Cold registration is explicit on additional producer threads.
pub fn register_thread() -> Result<(), CaptureError> {
    let mut registry = REGISTRY.lock().unwrap();
    if !registry.owner || !is_enabled() {
        return Err(CaptureError::Inactive);
    }
    TRACK.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.as_ref().is_some_and(|r| r.0.epoch == registry.epoch) {
            return Ok(());
        }
        let id = registry
            .next_track
            .checked_add(1)
            .ok_or(CaptureError::Exhausted)?;
        let buffer = Buffer::new(registry.capacity)?;
        let track = Arc::new(Track {
            epoch: registry.epoch,
            id,
            buffer: Mutex::new(buffer),
        });
        registry
            .tracks
            .try_reserve(1)
            .map_err(|_| CaptureError::Capacity)?;
        registry.next_track = id;
        registry.tracks.push(track.clone());
        *slot = Some(Registration(track));
        Ok(())
    })
}

pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

pub(super) fn next_scope() -> Option<super::SubmissionId> {
    if !is_enabled() {
        return None;
    }
    let mut registry = REGISTRY.lock().unwrap();
    let next = registry.next_scope.checked_add(1)?;
    registry.next_scope = next;
    Some(super::SubmissionId {
        capture: registry.epoch,
        value: next,
    })
}

pub(super) fn operation() -> Option<super::SubmissionId> {
    OPERATION.with(core::cell::Cell::get)
}

pub(super) fn invalid_scope(epoch: u64) {
    TRACK.with(|slot| {
        if let Some(track) = slot.borrow().as_ref().map(|r| &r.0)
            && track.epoch == epoch
        {
            track.buffer.lock().unwrap().invalid_scope();
        }
    });
}

pub(super) fn replace_operation(id: Option<super::SubmissionId>) -> Option<super::SubmissionId> {
    OPERATION.with(|slot| slot.replace(id))
}

pub(super) fn record(epoch: u64, data: super::RecordData) {
    if !is_enabled() {
        return;
    }
    let recorded = TRACK.with(|slot| {
        let slot = slot.borrow();
        let Some(track) = slot.as_ref().map(|r| &r.0) else {
            return false;
        };
        if track.epoch != epoch {
            return false;
        }
        track.buffer.lock().unwrap().record(data, true);
        true
    });
    if !recorded {
        let mut registry = REGISTRY.lock().unwrap();
        registry.unregistered = registry.unregistered.saturating_add(1);
    }
}

pub(super) struct Ticket {
    track: Weak<Track>,
}

pub(super) fn enter() -> Option<(Ticket, u8)> {
    if !is_enabled() {
        return None;
    }
    let (result, unregistered) = TRACK.with(|slot| {
        let slot = slot.borrow();
        let Some(registration) = slot.as_ref() else {
            return (None, true);
        };
        let track = &registration.0;
        let mut buffer = track.buffer.lock().unwrap();
        let unregistered = buffer.closed;
        let result = buffer.enter().map(|depth| {
            (
                Ticket {
                    track: Arc::downgrade(track),
                },
                depth,
            )
        });
        (result, unregistered)
    });
    if unregistered {
        let mut registry = REGISTRY.lock().unwrap();
        registry.unregistered = registry.unregistered.saturating_add(1);
    }
    result
}

pub(super) fn exit(ticket: Ticket, event: PerfEvent, valid: bool) -> bool {
    ticket
        .track
        .upgrade()
        .is_some_and(|track| track.buffer.lock().unwrap().exit(event, valid))
}

pub(super) fn stop(epoch: u64) {
    let registry = REGISTRY.lock().unwrap();
    if registry.epoch != epoch || !registry.owner {
        return;
    }
    ENABLED.store(false, Ordering::Relaxed);
    for track in &registry.tracks {
        track.buffer.lock().unwrap().closed = true;
    }
}

pub(super) fn release(epoch: u64) {
    stop(epoch);
    let mut registry = REGISTRY.lock().unwrap();
    if registry.epoch == epoch {
        registry.owner = false;
        for track in &registry.tracks {
            track.buffer.lock().unwrap().release_storage();
        }
        registry.tracks = Vec::new();
    }
    // Release this thread's old allocation. Other threads release at registration or exit.
    TRACK.with(|slot| {
        slot.borrow_mut().take();
    });
}

pub(super) fn drain(
    epoch: u64,
    scratch: &mut [super::Record],
    visitor: &mut impl FnMut(BatchInfo, &[super::Record]),
) {
    let (count, registered, unregistered) = {
        let mut registry = REGISTRY.lock().unwrap();
        if registry.epoch != epoch || !registry.owner {
            return;
        }
        (
            registry.tracks.len(),
            registry.next_track,
            core::mem::take(&mut registry.unregistered),
        )
    };
    for index in 0..count {
        let track = REGISTRY.lock().unwrap().tracks[index].clone();
        let (mut left, status) = track.buffer.lock().unwrap().snapshot();
        let info = BatchInfo {
            capture: epoch,
            track: track.id,
            process: std::process::id(),
            registered_tracks: registered,
            status,
        };
        while left != 0 {
            let limit = left.min(scratch.len());
            let n = track.buffer.lock().unwrap().read(&mut scratch[..limit]);
            if n == 0 {
                break;
            }
            visitor(info, &scratch[..n]);
            left -= n;
        }
        visitor(info, &[]);
    }
    visitor(
        BatchInfo {
            capture: epoch,
            track: 0,
            process: std::process::id(),
            registered_tracks: registered,
            status: TraceStatus {
                dropped: unregistered,
                ..TraceStatus::default()
            },
        },
        &[],
    );
    REGISTRY.lock().unwrap().tracks.retain(|track| {
        let buffer = track.buffer.lock().unwrap();
        !buffer.closed || !buffer.empty() || Arc::strong_count(track) > 1
    });
}
