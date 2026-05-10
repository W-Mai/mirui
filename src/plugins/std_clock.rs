use alloc::boxed::Box;

use crate::app::{App, RendererFactory};
use crate::backend::Backend;
use crate::plugin::Plugin;

/// Installs a monotonic clock based on `std::time::Instant`. Mutates
/// `App::clock` during build so every subsequent `post_render` hook sees
/// real nanoseconds since App start.
#[derive(Default)]
pub struct StdInstantClockPlugin;

impl<B, F> Plugin<B, F> for StdInstantClockPlugin
where
    B: Backend,
    F: RendererFactory<B>,
{
    fn build(&mut self, app: &mut App<B, F>) {
        let start = std::time::Instant::now();
        app.clock = Box::new(move || start.elapsed().as_nanos() as u64);
    }
}
