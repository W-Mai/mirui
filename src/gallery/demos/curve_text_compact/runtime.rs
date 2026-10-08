use super::composition::build_widgets;
use super::geometry::{WAVE, text_path};
use super::state::{CompactCurveMotion, CompactCurveNodes};
#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::ecs::DeltaTimeMs;
use crate::prelude::*;
use crate::ui::property::prop;

#[mirui_macros::system(order = Animation)]
pub(super) fn compact_curve_animation_system(
    delta: res!(DeltaTimeMs),
    motion: res!(mut CompactCurveMotion),
    nodes: res!(CompactCurveNodes),
    commands: commands!(),
) {
    let dt = delta.0.min(50);
    let phase = motion.0.advance(
        dt,
        520,
        Fixed::from_int(90),
        Fixed::ONE,
        false,
        Fixed::from_int(360),
    );
    commands.set::<prop::TextPath>(nodes.text, text_path(nodes.path, phase));
}

pub fn install<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    crate::gallery::showcase_theme::install(app);
    let path = app
        .paths()
        .insert_static(WAVE.commands())
        .expect("compact curve path");
    app.add_resource(CompactCurveMotion::default());
    app.add_system(compact_curve_animation_system::system());
    app.compose(parent, build_widgets(path));
    let text = app
        .world
        .find_by_id("compact_curve_text_primary")
        .expect("compact curve text");
    app.add_resource(CompactCurveNodes { path, text });
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
