#[cfg(any(feature = "std", test))]
use super::FONT_BYTES;
#[cfg(feature = "std")]
use super::composition::build_widgets;
use super::state::ConsoleModel;
#[cfg(feature = "std")]
use super::state::{ConsoleState, DemoRunMode};
#[cfg(feature = "std")]
use super::style::BG;
#[cfg(feature = "std")]
use super::visuals::{activity_view, backdrop_view, orbit_view, signal_view};
#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::ecs::DeltaTimeMs;
use crate::prelude::*;
#[cfg(any(feature = "std", test))]
use crate::render::font::{Font, FontManager};

#[cfg(any(feature = "std", test))]
pub(super) fn register_fonts(world: &mut World) {
    let Some(manager) = world.resource::<FontManager>() else {
        return;
    };
    let base = Font::from_mirx(
        "Orbit Console",
        14,
        FONT_BYTES,
        &mirx::reader::PayloadLimits::HOST,
    )
    .expect("Orbit Console font must decode");
    let mut body = base.clone();
    body.size = 14;
    let mut heading = base.clone();
    heading.size = 24;
    let mut mono = base;
    mono.size = 11;
    manager.add_static(FontToken::Default.cache_key(), body);
    manager.add_static(FontToken::Heading.cache_key(), heading);
    manager.add_static(FontToken::Mono.cache_key(), mono);
}

#[mirui_macros::system(order = ANIMATION)]
pub fn console_animation_system(world: &mut World) {
    let delta_ms = world.resource::<DeltaTimeMs>().map_or(16, |delta| delta.0);
    if let Some(model) = world.resource::<ConsoleModel>().cloned() {
        model.advance(delta_ms);
    }
}

#[cfg(feature = "std")]
pub fn setup<B, F>(app: &mut App<B, F>, parent: Entity, run_mode: DemoRunMode)
where
    B: Surface,
    F: RendererFactory<B>,
{
    crate::gallery::showcase_theme::install(&mut app.world);
    let initial_state = match run_mode {
        DemoRunMode::Live => ConsoleState::live(),
        DemoRunMode::Capture => ConsoleState::capture(),
    };
    app.world.insert_resource(ConsoleModel::new(initial_state));
    register_fonts(&mut app.world);
    if let Some(style) = app.world.get_mut::<Style>(parent) {
        style.set_bg_color(BG);
    }
    app.with_widget(backdrop_view())
        .with_widget(orbit_view())
        .with_widget(signal_view())
        .with_widget(activity_view());
    if run_mode == DemoRunMode::Live {
        if app.world.resource::<MonoClock>().is_none() {
            app.add_plugin(StdInstantClockPlugin);
        }
        app.add_system(console_animation_system::system());
    }
    app.compose(parent, build_widgets);
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    setup(app, parent, DemoRunMode::Live);
}
