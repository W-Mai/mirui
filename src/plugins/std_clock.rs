use crate::app::{App, RendererFactory};
use crate::ecs::MonoClock;
use crate::plugin::Plugin;
use crate::surface::Surface;

use std::sync::OnceLock;
use std::time::Instant;

static CLOCK_START: OnceLock<Instant> = OnceLock::new();

fn std_clock_ns() -> u64 {
    CLOCK_START.get_or_init(Instant::now).elapsed().as_nanos() as u64
}

/// Backs `MonoClock` with `std::time::Instant`. Required on desktop /
/// std builds for any plugin or system that consumes `MonoClock` (e.g.
/// scheduler perf timing, animation `dt`).
///
/// **Inserts**
/// - resource: `MonoClock` (clock fn returning ns since first build call)
/// - system: none
/// - view: none
/// - entity: none
/// - hooks: none
#[derive(Default)]
pub struct StdInstantClockPlugin;

impl<B, F> Plugin<B, F> for StdInstantClockPlugin
where
    B: Surface,
    F: RendererFactory<B>,
{
    fn build(&mut self, app: &mut App<B, F>) {
        CLOCK_START.get_or_init(Instant::now);
        app.world.insert_resource(MonoClock::new(std_clock_ns));
    }
}
