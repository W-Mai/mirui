use super::composition::build_widgets;
use super::render::field_view;
use super::state::Swarm;
use crate::app::plugins::StdInstantClockPlugin;
use crate::prelude::*;

/// One step of the dots every 16 ms.
#[mirui_macros::timer(every = 16)]
fn step(swarm: model!(Swarm)) {
    swarm.tick();
}

pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    crate::gallery::showcase_theme::install_sample(app);
    app.add_plugin(StdInstantClockPlugin);
    app.with_widget(field_view());
    let swarm = app.add_model(Swarm::default()).finish();
    app.add_timer(step::timer(swarm.clone()));
    app.compose(parent, build_widgets(swarm));
}
