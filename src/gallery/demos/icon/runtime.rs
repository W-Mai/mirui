use super::composition::build_widgets;
use super::motion::IconScale;
use crate::app::plugins::StdInstantClockPlugin;
use crate::prelude::*;

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.add_system(IconScale::system());
    app.add_plugin(StdInstantClockPlugin);
    app.compose(parent, build_widgets);
}
