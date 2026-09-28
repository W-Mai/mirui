#[cfg(feature = "std")]
use super::composition::build_widgets;
#[cfg(feature = "std")]
use super::motion::{AnimateColor, AnimateX};
#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
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
    app.add_system(AnimateX::system());
    app.add_system(AnimateColor::system());
    app.add_plugin(StdInstantClockPlugin)
        .add_plugin(FpsSummaryPlugin::default());
    app.compose(parent, build_widgets);
}
