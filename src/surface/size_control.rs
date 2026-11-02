//! Optional host content-size requests with reactive actual-size feedback.

use alloc::rc::Rc;
use core::cell::Cell;

use crate::core::reactive::Signal;

/// A host size request could not be applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceSizeError {
    InvalidSize,
    Unavailable,
    Restricted,
    Backend,
}

/// Logical content dimensions and the latest request's status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SurfaceSizeState {
    pub actual: (u16, u16),
    pub requested: Option<(u16, u16)>,
    pub error: Option<SurfaceSizeError>,
    pub available: bool,
}

/// Opaque request returned to the owning host for execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SurfaceSizeRequest {
    generation: u64,
    size: (u16, u16),
}

impl SurfaceSizeRequest {
    pub fn size(self) -> (u16, u16) {
        self.size
    }
}

struct Shared {
    state: Signal<SurfaceSizeState>,
    pending: Cell<Option<SurfaceSizeRequest>>,
    generation: Cell<u64>,
    alive: Cell<bool>,
}

impl Shared {
    fn publish(&self, state: SurfaceSizeState) {
        if self.state.get_untracked() != state {
            self.state.set(state);
        }
    }
}

/// Cloneable request metadata. Native window operations stay with the owning host.
#[derive(Clone)]
pub struct SurfaceSizeControl(Rc<Shared>);

impl SurfaceSizeControl {
    /// Record the latest desired logical content size. Acceptance does not confirm a resize.
    pub fn request_size(&self, width: u16, height: u16) -> Result<(), SurfaceSizeError> {
        if width == 0 || height == 0 {
            return Err(SurfaceSizeError::InvalidSize);
        }
        if !self.0.alive.get() {
            return Err(SurfaceSizeError::Unavailable);
        }
        let mut state = self.0.state.get_untracked();
        let size = (width, height);
        if state.requested == Some(size) {
            return Ok(());
        }
        let generation = self.0.generation.get().wrapping_add(1);
        self.0.generation.set(generation);
        state.error = None;
        if state.actual == size && state.requested.is_none() {
            self.0.pending.set(None);
            state.requested = None;
        } else {
            self.0
                .pending
                .set(Some(SurfaceSizeRequest { generation, size }));
            state.requested = Some(size);
        }
        self.0.publish(state);
        Ok(())
    }

    /// Observe actual logical dimensions, pending work and host rejection.
    pub fn state(&self) -> SurfaceSizeState {
        let mut state = self.0.state.get();
        if !self.0.alive.get() {
            state.available = false;
            state.requested = None;
            state.error = Some(SurfaceSizeError::Unavailable);
        }
        state
    }

    pub fn state_untracked(&self) -> SurfaceSizeState {
        let mut state = self.0.state.get_untracked();
        if !self.0.alive.get() {
            state.available = false;
            state.requested = None;
            state.error = Some(SurfaceSizeError::Unavailable);
        }
        state
    }
}

/// Owner-thread endpoint for a resizable surface. Create it only when size control is requested.
pub struct SurfaceSizeHost(Rc<Shared>);

impl SurfaceSizeHost {
    pub fn new(width: u16, height: u16) -> Self {
        Self(Rc::new(Shared {
            state: Signal::new(SurfaceSizeState {
                actual: (width, height),
                requested: None,
                error: None,
                available: true,
            }),
            pending: Cell::new(None),
            generation: Cell::new(0),
            alive: Cell::new(true),
        }))
    }

    pub fn control(&self) -> SurfaceSizeControl {
        SurfaceSizeControl(self.0.clone())
    }

    /// Take the latest request once; a superseded request is never replayed.
    pub fn take_request(&self) -> Option<SurfaceSizeRequest> {
        self.0.pending.take()
    }

    /// Publish actual host geometry without completing a pending request.
    pub fn observe_size(&self, width: u16, height: u16) {
        if width == 0 || height == 0 {
            return;
        }
        let mut state = self.0.state.get_untracked();
        state.actual = (width, height);
        self.0.publish(state);
    }

    /// Complete an executed request. Outdated completions cannot complete its replacement.
    pub fn complete(
        &self,
        request: SurfaceSizeRequest,
        result: Result<(u16, u16), SurfaceSizeError>,
    ) {
        if request.generation != self.0.generation.get() {
            return;
        }
        let mut state = self.0.state.get_untracked();
        if state.requested != Some(request.size) {
            return;
        }
        self.0.pending.set(None);
        state.requested = None;
        state.error = match result {
            Ok((width, height)) if width > 0 && height > 0 => {
                state.actual = (width, height);
                (request.size != state.actual).then_some(SurfaceSizeError::Restricted)
            }
            Ok(_) => Some(SurfaceSizeError::InvalidSize),
            Err(error) => Some(error),
        };
        self.0.publish(state);
    }
}

impl Drop for SurfaceSizeHost {
    fn drop(&mut self) {
        self.0.alive.set(false);
        self.0.pending.set(None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_coalesce_and_old_completions_cannot_acknowledge_the_latest_intent() {
        let host = SurfaceSizeHost::new(640, 480);
        let control = host.control();
        control.request_size(940, 480).unwrap();
        let old = host.take_request().unwrap();
        assert_eq!(control.state().actual, (640, 480));
        control.request_size(1100, 600).unwrap();
        let latest = host.take_request().unwrap();
        host.complete(old, Ok((940, 480)));
        assert_eq!(control.state().requested, Some((1100, 600)));
        host.complete(latest, Ok((1100, 600)));
        assert_eq!(control.state().actual, (1100, 600));
        assert_eq!(control.state().requested, None);
        assert!(host.take_request().is_none());
    }

    #[test]
    fn cancellation_restriction_and_owner_retirement_are_observable() {
        let host = SurfaceSizeHost::new(640, 480);
        let control = host.control();
        assert_eq!(
            control.request_size(0, 480),
            Err(SurfaceSizeError::InvalidSize)
        );
        control.request_size(940, 480).unwrap();
        let old = host.take_request().unwrap();
        control.request_size(640, 480).unwrap();
        host.complete(old, Ok((940, 480)));
        assert_eq!(control.state().actual, (640, 480));
        assert_eq!(control.state().requested, Some((640, 480)));
        let closed = host.take_request().unwrap();
        host.complete(closed, Ok((640, 480)));
        control.request_size(1100, 480).unwrap();
        let request = host.take_request().unwrap();
        host.complete(request, Ok((900, 480)));
        assert_eq!(control.state().error, Some(SurfaceSizeError::Restricted));
        assert_eq!(control.state().actual, (900, 480));
        drop(host);
        assert!(!control.state().available);
        assert_eq!(
            control.request_size(940, 480),
            Err(SurfaceSizeError::Unavailable)
        );
    }

    #[test]
    fn metadata_requests_and_feedback_reuse_their_storage() {
        let host = SurfaceSizeHost::new(640, 480);
        let control = host.control();
        for _ in 0..2 {
            assert_eq!(
                crate::test_support::tracked_allocations(|| {
                    control.request_size(940, 480).unwrap();
                    let request = host.take_request().unwrap();
                    host.observe_size(940, 480);
                    host.complete(request, Ok((940, 480)));
                    control.request_size(640, 480).unwrap();
                    let request = host.take_request().unwrap();
                    host.complete(request, Ok((640, 480)));
                }),
                0
            );
        }
    }
}
