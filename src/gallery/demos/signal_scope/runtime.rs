use super::HOUSING_ART;
use super::controls::build_widgets;
use super::render::{ScopeWaveScratch, scope_view};
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

#[mirui_macros::system(order = ANIMATION)]
fn scope_animation_system(world: &mut World) {
    let delta = world.resource::<DeltaTimeMs>().map_or(16, |delta| delta.0);
    let Some(model) = world.resource::<ScopeModel>().cloned() else {
        return;
    };
    let mut state = model.snapshot();
    if state.tick(delta) {
        model.state.set(state);
    }
}

pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    crate::gallery::showcase_theme::install(&mut app.world);
    register_fonts(&mut app.world);
    app.world.insert_resource(ScopeModel::default());
    app.world.insert_resource(ScopeWaveScratch::default());
    app.add_plugin(StdInstantClockPlugin)
        .add_plugin(
            crate::app::plugins::ImageResourcesPlugin::empty().with_mirx_bytes_options(
                "signal_scope_housing",
                HOUSING_ART,
                housing_texture_options(),
            ),
        )
        .with_widget(scope_view())
        .add_system(scope_animation_system::system());
    app.compose(parent, build_widgets);
}
