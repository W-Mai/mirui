use crate::anim::FrameClock;
use crate::app::{App, RendererFactory};
use crate::plugin::Plugin;
use crate::surface::Surface;

use std::sync::OnceLock;
use std::time::Instant;

static CLOCK_START: OnceLock<Instant> = OnceLock::new();

fn std_clock_ns() -> u64 {
    CLOCK_START.get_or_init(Instant::now).elapsed().as_nanos() as u64
}

#[derive(Default)]
pub struct StdInstantClockPlugin;

impl<B, F> Plugin<B, F> for StdInstantClockPlugin
where
    B: Surface,
    F: RendererFactory<B>,
{
    fn build(&mut self, app: &mut App<B, F>) {
        CLOCK_START.get_or_init(Instant::now);
        app.world.insert_resource(FrameClock::new(std_clock_ns));
    }
}
