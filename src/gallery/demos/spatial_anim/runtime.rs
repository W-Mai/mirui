#[cfg(feature = "std")]
use super::composition::build_widgets;
#[cfg(feature = "std")]
use super::motion::{AnimateTweenY, spring_system};
#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
#[cfg(feature = "std")]
use crate::ecs::Entity;
#[cfg(feature = "std")]
use crate::prelude::plugin::FpsSummaryPlugin;
#[cfg(feature = "std")]
use crate::prelude::*;

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.add_system(AnimateTweenY::system());
    app.add_system(spring_system::system());
    app.add_plugin(StdInstantClockPlugin)
        .add_plugin(FpsSummaryPlugin::default());
    app.compose(parent, build_widgets);
}
