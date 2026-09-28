#[cfg(feature = "std")]
use super::animation::{BlurPan, GlowPulse, ShadowOffset, animate_color_flash};
#[cfg(feature = "std")]
use super::composition::build_widgets;
#[cfg(feature = "std")]
use crate::prelude::*;

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::app::plugins::StdInstantClockPlugin;
    app.add_plugin(StdInstantClockPlugin)
        .with_offscreen_pool_budget(1024 * 1024)
        .add_system(animate_color_flash::system())
        .add_system(BlurPan::system())
        .add_system(ShadowOffset::system())
        .add_system(GlowPulse::system());
    app.compose(parent, build_widgets);
}
