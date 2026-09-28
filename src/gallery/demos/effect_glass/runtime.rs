#[cfg(feature = "std")]
use super::composition::build_widgets;
#[cfg(feature = "std")]
use super::motion::{GaussRadius, GlassX};
#[cfg(feature = "std")]
use crate::prelude::*;

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::app::plugins::StdInstantClockPlugin;

    app.add_plugin(StdInstantClockPlugin);
    app.add_system(GlassX::system());
    app.add_system(GaussRadius::system());
    app.with_offscreen_pool_budget(8 * 1024);
    app.compose(parent, build_widgets);
}
