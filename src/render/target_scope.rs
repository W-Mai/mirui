use crate::render::renderer::{RenderError, Renderer};
use crate::types::Viewport;

use super::BackendTarget;

pub use alloc::boxed::Box;

/// Object-safe entry to the concrete renderer's shared target scope.
///
/// Implemented automatically for renderers. Backend authors implement the target binding methods on `Renderer`.
pub trait TargetScopeDispatch {
    fn dispatch_target_scope(
        &mut self,
        target: &mut dyn BackendTarget,
        viewport: Viewport,
        workspace: &mut [u8],
        body: &mut dyn FnMut(&mut dyn Renderer) -> Result<(), RenderError>,
    ) -> Result<(), RenderError>;
}

struct TargetScope<'a, R: Renderer> {
    renderer: &'a mut R,
    state: Option<R::TargetState>,
    _target: &'a mut dyn BackendTarget,
}

impl<R: Renderer> Drop for TargetScope<'_, R> {
    fn drop(&mut self) {
        if let Some(state) = self.state.take() {
            // A backend must latch restoration failure even during unwinding.
            let _ = self.renderer.restore_target(state, false);
        }
    }
}

impl<R: Renderer> TargetScopeDispatch for R {
    fn dispatch_target_scope(
        &mut self,
        target: &mut dyn BackendTarget,
        viewport: Viewport,
        workspace: &mut [u8],
        body: &mut dyn FnMut(&mut dyn Renderer) -> Result<(), RenderError>,
    ) -> Result<(), RenderError> {
        self.flush()?;
        target.validate()?;
        let state = self.bind_target(target, viewport, workspace)?;
        let mut scope = TargetScope {
            renderer: self,
            state: Some(state),
            _target: target,
        };
        let result = body(scope.renderer).and_then(|()| scope.renderer.flush());
        let state = scope.state.take().expect("active target scope");
        let restored = scope.renderer.restore_target(state, result.is_ok());
        result.and(restored)
    }
}
