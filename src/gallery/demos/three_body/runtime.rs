use super::composition::build_widgets;
use super::render::field_view;
use super::state::Orbit;
use crate::app::plugins::StdInstantClockPlugin;
use crate::prelude::*;

/// One step of the simulation every 16 ms.
#[mirui_macros::timer(every = 16)]
fn step(orbit: model!(Orbit)) {
    orbit.tick();
}

pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    crate::gallery::showcase_theme::install_sample(app);
    app.add_plugin(StdInstantClockPlugin);
    app.with_widget(field_view());
    let orbit = app.add_model(Orbit::default()).finish();
    app.add_timer(step::timer(orbit.clone()));
    app.compose(parent, build_widgets(orbit));
}
