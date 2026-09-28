use super::composition::build_widgets;
use super::geometry::{WAVE, text_path};
use super::state::{CompactCurveMotion, CompactCurveNodes};
#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::ecs::DeltaTimeMs;
use crate::prelude::*;
use crate::render::path::PathStore;

#[mirui_macros::system(order = ANIMATION)]
pub(super) fn compact_curve_animation_system(world: &mut World) {
    let dt = world
        .resource::<DeltaTimeMs>()
        .map_or(16, |delta| delta.0)
        .min(50);
    let Some(motion) = world.resource_mut::<CompactCurveMotion>() else {
        return;
    };
    let phase = motion.0.advance(
        dt,
        520,
        Fixed::from_int(90),
        Fixed::ONE,
        false,
        Fixed::from_int(360),
    );
    let Some(nodes) = world.resource::<CompactCurveNodes>().copied() else {
        return;
    };
    if let Some(mut text) = world.widget_mut(nodes.text) {
        text.text_path(text_path(nodes.path, phase));
    }
}

pub fn install<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    crate::gallery::showcase_theme::install(&mut app.world);
    let path = app
        .world
        .resource_mut::<PathStore>()
        .expect("path store")
        .insert_static(WAVE.commands())
        .expect("compact curve path");
    app.world.insert_resource(CompactCurveMotion::default());
    app.add_system(compact_curve_animation_system::system());
    app.compose(parent, |cx| build_widgets(cx, path));
    let text = app
        .world
        .find_by_id("compact_curve_text_primary")
        .expect("compact curve text");
    app.world.insert_resource(CompactCurveNodes { path, text });
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
