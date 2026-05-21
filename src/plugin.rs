use crate::app::{App, RendererFactory};
use crate::ecs::World;
use crate::surface::{InputEvent, Surface};

/// Extend `App` with cross-cutting behaviour — stats, custom clock, logging,
/// hotkeys — without forking the run loop. `build` runs once at registration;
/// the other methods are called by `App` at the matching point in the frame
/// schedule. Any `FnMut(&mut App<B, F>)` is also a plugin via the blanket impl.
///
/// # Documentation contract
///
/// Every plugin's docstring must list what it inserts, in this format
/// (omit empty rows):
///
/// ```text
/// **Inserts**
/// - resource: <names or "none">
/// - system:   <names + slot, or "none">
/// - view:     <names + priority, or "none">
/// - entity:   <markers + spawn timing, or "none">
/// - hooks:    <`on_event` / `pre_render` / `post_render` / `on_quit`, or "none">
/// ```
///
/// Users reading `add_plugin(...)` can then tell what changes in their
/// `World` without reading the plugin source. Existing built-in plugins
/// (`PerfReportPlugin`, `FpsSummaryPlugin`, `StdInstantClockPlugin`,
/// `InputFeedbackPlugin`) follow this contract; user-defined plugins are
/// expected to as well.
pub trait Plugin<B, F>
where
    B: Surface,
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
    B: Surface,
    F: RendererFactory<B>,
    G: FnMut(&mut App<B, F>) + 'static,
{
    fn build(&mut self, app: &mut App<B, F>) {
        (self)(app)
    }
}
