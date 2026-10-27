use super::{BatchInfo, CaptureError, PerfEvent, buffer::Buffer};
use core::sync::atomic::{AtomicBool, Ordering};

static ENABLED: AtomicBool = AtomicBool::new(false);
struct State {
    epoch: u64,
    buffer: Option<Buffer>,
}
static mut STATE: State = State {
    epoch: 0,
    buffer: None,
};

pub(super) const OVERHEAD: usize = core::mem::size_of::<State>();

fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    critical_section::with(|_| {
        #[allow(static_mut_refs)]
        unsafe {
            f(&mut STATE)
        }
    })
}

pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}
pub fn register_thread() -> Result<(), CaptureError> {
    if is_enabled() {
        Ok(())
    } else {
        Err(CaptureError::Inactive)
    }
}

pub(super) fn start(capacity: usize) -> Result<u64, CaptureError> {
    // Allocation is cold and outside the critical section.
    let buffer = Buffer::new(capacity)?;
    with_state(|state| {
        if state.buffer.is_some() {
            return Err(CaptureError::InUse);
        }
        state.epoch = state.epoch.checked_add(1).ok_or(CaptureError::Exhausted)?;
        state.buffer = Some(buffer);
        ENABLED.store(true, Ordering::Relaxed);
        Ok(state.epoch)
    })
}

pub(super) struct Ticket(core::num::NonZeroU64);
pub(super) fn enter() -> Option<(Ticket, u8)> {
    if !is_enabled() {
        return None;
    }
    with_state(|state| {
        Some((
            Ticket(core::num::NonZeroU64::new(state.epoch)?),
            state.buffer.as_mut()?.enter()?,
        ))
    })
}
pub(super) fn exit(ticket: Ticket, event: PerfEvent, valid: bool) -> bool {
    with_state(|state| {
        state.epoch == ticket.0.get()
            && state
                .buffer
                .as_mut()
                .is_some_and(|buffer| buffer.exit(event, valid))
    })
}
pub(super) fn stop(epoch: u64) {
    with_state(|state| {
        if state.epoch == epoch {
            ENABLED.store(false, Ordering::Relaxed);
            if let Some(buffer) = &mut state.buffer {
                buffer.closed = true;
            }
        }
    });
}
pub(super) fn release(epoch: u64) {
    stop(epoch);
    let old = with_state(|state| {
        if state.epoch == epoch {
            state.buffer.take()
        } else {
            None
        }
    });
    drop(old);
}
pub(super) fn drain(
    epoch: u64,
    scratch: &mut [PerfEvent],
    visitor: &mut impl FnMut(BatchInfo, &[PerfEvent]),
) {
    let Some((mut left, status)) = with_state(|state| {
        (state.epoch == epoch)
            .then(|| state.buffer.as_mut().map(Buffer::snapshot))
            .flatten()
    }) else {
        return;
    };
    let info = BatchInfo {
        capture: epoch,
        track: 1,
        process: 1,
        registered_tracks: 1,
        status,
    };
    while left != 0 {
        let limit = left.min(scratch.len());
        let n = with_state(|state| state.buffer.as_mut().unwrap().read(&mut scratch[..limit]));
        if n == 0 {
            break;
        }
        visitor(info, &scratch[..n]);
        left -= n;
    }
    visitor(info, &[]);
}
