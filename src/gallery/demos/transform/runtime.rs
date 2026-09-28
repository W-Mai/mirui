use super::composition::build_widgets;
use super::motion::spin_system;
use crate::prelude::*;

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.add_system(spin_system::system());
    app.add_plugin(crate::app::plugins::ImageResourcesPlugin::default());
    app.compose(parent, build_widgets);
}
