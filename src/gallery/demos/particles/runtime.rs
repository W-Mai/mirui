use super::composition::build_widgets;
use super::motion::{bar_system, particle_bounds_system, particle_system, pulse_ring_system};
use crate::app::plugins::StdInstantClockPlugin;
use crate::prelude::*;

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.add_plugin(StdInstantClockPlugin);
    app.add_system(particle_bounds_system::system());
    app.add_system(particle_system::system());
    app.add_system(pulse_ring_system::system());
    app.add_system(bar_system::system());
    app.compose(parent, build_widgets);
}
