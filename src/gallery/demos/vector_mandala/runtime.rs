use super::composition::build_widgets;
use super::render::{vector_mandala_anim_system, vector_mandala_view};
use crate::app::plugins::StdInstantClockPlugin;
use crate::prelude::*;

pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.add_plugin(StdInstantClockPlugin);
    app.with_widget(vector_mandala_view());
    app.add_system(vector_mandala_anim_system::system());
    app.compose(parent, build_widgets);
}
