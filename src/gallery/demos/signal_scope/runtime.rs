use super::HOUSING_ART;
use super::controls::build_widgets;
use super::render::{ScopeWaveScratch, scope_grid_render, scope_view};
use super::state::ScopeModel;
use crate::app::plugins::StdInstantClockPlugin;
use crate::ecs::DeltaTimeMs;
use crate::gallery::demos::instruments::register_fonts;
use crate::prelude::*;
use crate::render::texture::MirxTextureOptions;

pub(super) const fn housing_texture_options() -> MirxTextureOptions {
    MirxTextureOptions::new()
        .with_limits(mirx::reader::PayloadLimits::EMBEDDED.with_max_decoded_bytes(800 * 480 * 2))
}

#[mirui_macros::system(order = ANIMATION, bind(model))]
pub(super) fn scope_animation_system(model: &ScopeModel, delta: DeltaTimeMs) {
    model.advance_ms(delta.0);
}

pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    crate::gallery::showcase_theme::install(&mut app.world);
    register_fonts(&mut app.world);
    app.world.insert_resource(ScopeWaveScratch::default());
    let model = app.add_model(ScopeModel::default());
    app.add_plugin(StdInstantClockPlugin)
        .add_plugin(
            crate::app::plugins::ImageResourcesPlugin::empty().with_mirx_bytes_options(
                "signal_scope_housing",
                HOUSING_ART,
                housing_texture_options(),
            ),
        )
        .with_widget(scope_grid_render::view())
        .with_widget(scope_view())
        .add_system(scope_animation_system::system(model.clone()));
    app.compose(parent, |cx| build_widgets(cx, model));
}
