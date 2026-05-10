use crate::app::{App, RendererFactory};
use crate::backend::{Backend, InputEvent};
use crate::ecs::World;

/// Extend `App` with cross-cutting behaviour — stats, custom clock, logging,
/// hotkeys — without forking the run loop. `build` runs once at registration;
/// the other methods are called by `App` at the matching point in the frame
/// schedule. Any `FnMut(&mut App<B, F>)` is also a plugin via the blanket impl.
pub trait Plugin<B, F>
where
    B: Backend,
    F: RendererFactory<B>,
{
    fn build(&mut self, app: &mut App<B, F>);

    fn pre_render(&mut self, _world: &mut World) {}

    /// Called after a successful render, with the measured render duration in
    /// nanoseconds. `render_nanos` will be 0 when no clock plugin has been
    /// installed — plugins that care about timing should either install one
    /// or skip their logic when they see 0.
    fn post_render(&mut self, _world: &mut World, _render_nanos: u64) {}

    /// Inspect each input event before it reaches widgets. Return true to
    /// mark the event consumed; `App` will skip widget-level dispatch for
    /// that event.
    fn on_event(&mut self, _world: &mut World, _event: &InputEvent) -> bool {
        false
    }

    fn on_quit(&mut self, _world: &mut World) {}

    fn name(&self) -> &'static str {
        core::any::type_name::<Self>()
    }
}

impl<B, F, G> Plugin<B, F> for G
where
    B: Backend,
    F: RendererFactory<B>,
    G: FnMut(&mut App<B, F>) + 'static,
{
    fn build(&mut self, app: &mut App<B, F>) {
        (self)(app)
    }
}
