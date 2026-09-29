mod composition;
mod geometry;
mod runtime;
mod stage;
mod state;
mod style;

#[cfg(test)]
mod tests;

#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::prelude::*;

pub const VIEWPORT: (u16, u16) = (960, 540);

pub fn install<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    crate::gallery::showcase_theme::install(&mut app.world);
    app.with_widget(stage::curve_stage_background_view())
        .with_widget(stage::curve_paths_view());
    crate::gallery::demos::typography_lab::register_fonts(&mut app.world);
    let model = app.add_model(state::CurveModel::default());
    let paths = geometry::register_paths(&mut app.world);
    app.world.insert_resource(paths);
    app.add_system(runtime::curve_text_animation_system::system());
    app.compose(parent, |cx| composition::build_widgets(cx, model, paths));
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    if app.world.resource::<MonoClock>().is_none() {
        app.add_plugin(StdInstantClockPlugin);
    }
    install(app, parent);
}

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::range(320, 320, 960, 540);
