use super::renderer::{DrawRequest, RenderError, RenderRoute, Renderer};

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

    /// Execute one request against the shared target.
    fn submit(&mut self, target: &mut T, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
        target.submit(request)
    }

    /// Finish pending work before another engine accesses the target. This is
    /// also called after a failed `submit`.
    fn end(&mut self, _target: &mut T) {}
}
