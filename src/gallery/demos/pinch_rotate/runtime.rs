use super::composition::build_widgets;
use super::state::PinchModel;
use crate::app::plugins::StdInstantClockPlugin;
use crate::input::event::sim::sim_timeline_system;
use crate::prelude::*;

pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    let model = app.add_model(PinchModel::default()).finish();
    app.compose(parent, build_widgets(model));

    app.add_system(sim_timeline_system::system());
    app.add_plugin(StdInstantClockPlugin);
}
