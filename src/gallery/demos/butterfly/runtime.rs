use super::animation::butterfly_anim_system;
use super::composition::build_widgets;
use super::render::butterfly_view;
use crate::app::plugins::StdInstantClockPlugin;
use crate::prelude::*;

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.add_plugin(StdInstantClockPlugin);
    app.with_widget(butterfly_view());
    app.add_system(butterfly_anim_system::system());
    app.compose(parent, build_widgets);
}
