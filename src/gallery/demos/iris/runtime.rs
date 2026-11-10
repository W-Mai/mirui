use super::FONT_BYTES;
use super::composition::build_widgets;
use super::lens::lens_render;
use super::meters::{
    ShutterRing, dial_render, exposure_render, focus_render, histogram_render, shutter_render,
};
use super::state::{IrisRun, IrisState};
use super::style::{self, FONT};
use crate::ecs::DeltaTimeMs;
use crate::prelude::*;
use crate::render::font::{Font, FontManager};

#[compose]
pub(super) fn register_fonts() {
    res!(FontManager).with(|manager| {
        let face = Font::from_mirx("Iris", 14, FONT_BYTES, &mirx::reader::PayloadLimits::HOST)
            .expect("Iris font must decode");
        manager.add_static(FONT.cache_key(), face);
    });
}

#[mirui_macros::system(order = Animation)]
pub(super) fn iris_animation_system(model: model!(IrisState), delta: res!(DeltaTimeMs)) {
    model.advance(delta.0);
}

/// Install Iris under `parent`. `Capture` loops the showcase path exactly so
/// README artwork can be recorded frame by frame.
pub fn setup<B, F>(app: &mut App<B, F>, parent: Entity, run: IrisRun)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.register_theme(style::dark());
    app.register_theme(style::light());
    app.set_theme(style::DARK_ID)
        .expect("iris theme registered");
    let (width, height) = app.backend.viewport().logical_size();
    let model = app.add_model(IrisState::new(run, width, height)).finish();
    app.compose(parent, register_fonts());
    crate::gallery::brand::install(app);
    app.with_widget(lens_render::view())
        .with_widget(histogram_render::view())
        .with_widget(exposure_render::view())
        .with_widget(focus_render::view())
        .with_widget(shutter_render::view())
        .with_widget(dial_render::view());
    #[cfg(feature = "std")]
    if run == IrisRun::Live {
        app.compose(parent, crate::gallery::demos::clock::ensure_clock());
    }
    app.add_system(iris_animation_system::system(model.clone()));
    app.compose(parent, build_widgets(model));
}

pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    setup(app, parent, IrisRun::Live);
}

/// Bounds of the shutter button in the active layout, for tools that script
/// or annotate a tap. Micro and dial faces have no on-screen shutter.
pub fn shutter_bounds(world: &crate::ecs::World) -> Option<Rect> {
    world
        .query::<ShutterRing>()
        .iter()
        .filter_map(|(entity, _)| world.get::<crate::ui::ComputedRect>(entity))
        .map(|rect| rect.0)
        .find(|rect| rect.w.is_positive() && rect.h.is_positive())
}
