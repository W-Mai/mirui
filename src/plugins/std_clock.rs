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

/// Backs `MonoClock` with `std::time::Instant`.
///
/// **Inserts**
/// - resource: `MonoClock`
/// - global: calls `crate::perf::set_clock` so `trace_span!` records
///   on `no_std` builds (no-op on `std` since the std imp uses
///   `Instant` directly)
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
        // std `perf` imp uses Instant directly, so this is a no-op
        // there; calling it keeps API symmetry with no_std clock plugins.
        crate::perf::set_clock(std_clock_ns);
    }
}
