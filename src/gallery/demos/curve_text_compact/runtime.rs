use super::composition::build_widgets;
use super::geometry::{WAVE, text_path};
use super::state::CompactCurveMotion;
use crate::ecs::DeltaTimeMs;
use crate::prelude::*;
use crate::text::TextPath;
use crate::ui::property::prop;

#[mirui_macros::system(order = Animation)]
pub(super) fn compact_curve_animation_system(
    delta: res!(DeltaTimeMs),
    texts: query!(Entity, mut CompactCurveMotion, TextPath),
    commands: commands!(),
) {
    let dt = delta.0.min(50);
    for (entity, motion, path) in texts {
        let phase = motion.0.advance(
            dt,
            520,
            Fixed::from_int(90),
            Fixed::ONE,
            false,
            Fixed::from_int(360),
        );
        commands.set::<prop::TextPath>(entity, text_path(path.path(), phase));
    }
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
    app.add_system(compact_curve_animation_system::system());
    app.compose(parent, build_widgets(path));
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.compose(parent, crate::gallery::demos::clock::ensure_clock());
    install(app, parent);
}
