use super::PreparedDraw;
use super::renderer::{DrawRequest, RenderError, RenderRoute, Renderer};
use core::task::{Context, Poll};

/// An execution engine that borrows a renderer's output target for each
/// ordered session.
///
/// The renderer owns the target. Engines may keep accelerator state, but they
/// must not own a second logical framebuffer. `begin` and `end` delimit
/// consecutive commands routed to the same engine, allowing cache and command
/// barriers to run once per transition.
pub trait RenderEngine<T: Renderer> {
    /// Classify a request without changing the target or engine state.
    fn route(&self, target: &T, request: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
        target.route(request)
    }

    /// Enter an ordered target session before a routed command. Returning an
    /// error must leave both engine and target ready for another submission.
    fn begin(&mut self, _target: &mut T) -> Result<(), RenderError> {
        Ok(())
    }

    /// Prepare sources through the same engine that will submit the command.
    fn prepare<'data>(
        &mut self,
        target: &mut T,
        request: DrawRequest<'_, 'data>,
        route: RenderRoute,
        cx: &mut Context<'_>,
    ) -> Poll<Result<PreparedDraw<'data>, RenderError>> {
        target.prepare(request, route, cx)
    }

    /// Execute one prepared command against the shared target.
    fn submit(
        &mut self,
        target: &mut T,
        draw: &::mirui::render::PreparedDraw<'_>,
    ) -> Result<(), RenderError> {
        target.submit(draw)
    }

    /// Execute a prepared text run while preserving the engine's ordinary submission semantics.
    fn submit_text_run(
        &mut self,
        target: &mut T,
        draw: &PreparedDraw<'_>,
        _identity: super::TextRunIdentity,
    ) -> Result<(), RenderError> {
        self.submit(target, draw)
    }

    /// Finish pending work before another engine accesses the target. This is
    /// also called after a failed `submit`.
    fn end(&mut self, _target: &mut T) {}
}
