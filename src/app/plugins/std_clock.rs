use crate::app::plugin::Plugin;
use crate::app::{App, RendererFactory};
use crate::ecs::MonoClock;
use crate::surface::Surface;

/// Publishes `core::time::clock_now_ns` as the `MonoClock` resource so
/// ECS systems can read frame timing without knowing about the process
/// clock. std auto-anchors its epoch on first read even without this
/// plugin — installing it is only about exposing time to ECS-level
/// consumers.
///
/// **Inserts**
/// - resource: `MonoClock`
#[derive(Default)]
pub struct StdInstantClockPlugin;

impl<B, F> Plugin<B, F> for StdInstantClockPlugin
where
    B: Surface,
    F: RendererFactory<B>,
{
    fn build(&mut self, app: &mut App<B, F>) {
        app.world
            .insert_resource(MonoClock::new(crate::core::time::clock_now_ns));
    }
}
