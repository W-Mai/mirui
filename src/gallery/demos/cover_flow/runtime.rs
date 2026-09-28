#[cfg(feature = "std")]
use super::composition::build_widgets;
#[cfg(feature = "std")]
use super::layout::layout_system;
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
    let info = app.backend.display_info();
    app.add_system(layout_system::system())
        .add_plugin(StdInstantClockPlugin)
        .add_plugin(FpsSummaryPlugin::default())
        .add_plugin(crate::app::plugins::ImageResourcesPlugin::default());
    app.compose(parent, |cx| build_widgets(cx, info.width, info.height));
}
