use super::composition::build_widgets;
use super::motion::{DriftX, DriftY, Flap};
use super::render::butterfly_view;
use crate::app::plugins::StdInstantClockPlugin;
use crate::prelude::*;

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    crate::gallery::showcase_theme::install_sample(app);
    app.add_plugin(StdInstantClockPlugin);
    app.with_widget(butterfly_view());
    app.add_system(Flap::system())
        .add_system(DriftX::system())
        .add_system(DriftY::system());
    app.compose(parent, build_widgets());
}
