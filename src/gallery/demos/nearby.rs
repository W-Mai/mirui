mod actions;
mod actor;
mod choices;
mod chrome;
mod composition;
mod consoles;
mod copy;
pub mod data;
mod feedback;
mod input;
pub mod launcher;
mod launcher_view;
pub mod model;
mod motion;
mod pages;
mod palette;
mod pixel;
mod sessions;
mod style;
mod widgets;

#[cfg(test)]
mod tests;

use crate::prelude::*;
use crate::render::font::{Font, FontManager};

pub fn setup_app<B: Surface, F: RendererFactory<B>>(
    app: &mut App<B, F>,
    parent: Entity,
) -> model::DisplayStateHandle {
    app.with_theme(palette::theme(0));
    let font = Font::from_mirx(
        "Nearby Pixel",
        12,
        include_bytes!("assets/nearby-menu.mirx"),
        &mirx::reader::PayloadLimits::HOST,
    )
    .expect("bundled Nearby font");
    let fonts = FontManager::new(crate::core::cache::MaxSize::Unbound, font.clone());
    fonts.add_static(style::FONT.cache_key(), font);
    fonts.add_static(
        style::TITLE_FONT.cache_key(),
        Font::from_mirx(
            "Nearby Pixel Title",
            24,
            include_bytes!("assets/nearby-title.mirx"),
            &mirx::reader::PayloadLimits::HOST,
        )
        .expect("bundled Nearby title font"),
    );
    app.with_fonts(fonts);
    app.with_offscreen_pool_budget(640 * 480 * 8);
    app.with_widget(crate::ui::widgets::page_view::view::<model::PageKey>());
    app.with_widget(crate::ui::widgets::page_view::view::<launcher::Screen>());
    app.with_widget(input::selection_target::view());
    app.with_widget(input::selection_frame::view());
    app.with_widget(pixel::scenery::view());
    app.with_widget(pixel::choice_sprite::view());
    app.with_widget(pixel::walker_sprite::view());
    app.with_widget(pixel::session_sprite::view());
    app.with_widget(pixel::signal_sprite::view());
    app.with_widget(pixel::search_sprite::view());
    app.with_widget(pixel::actor_sprite::view());
    #[cfg(feature = "std")]
    app.add_plugin(crate::app::plugins::StdInstantClockPlugin);
    let state = app.add_model(model::DisplayState::default()).finish();
    state.resize(app.backend.viewport().logical_size().1);
    app.add_system(launcher_hold::system(state.launcher()));
    app.add_system(launcher_palette::system(state.launcher()));
    app.add_system(scene_animation::system(state.clone()));
    app.add_system(actor::follow::system(state.clone()));
    app.on_event(
        crate::input::event::input::Key::EVENT,
        input::keyboard::callback(state.clone()),
    );
    app.compose(parent, composition::build_widgets(state.clone()));
    state
}

#[system(order = Animation)]
fn launcher_hold(library: model!(launcher::Launcher), delta: res!(crate::ecs::DeltaTimeMs)) {
    if library.active() && library.holding() && library.hold_progress() < Fixed::ONE {
        library.advance_hold(delta.0);
    }
}

#[system(order = Animation)]
fn launcher_palette(
    library: model!(launcher::Launcher),
    theme: res!(crate::ui::Theme),
    commands: commands!(),
) {
    if theme.id().as_str() != ["forest", "sunset", "night"][library.palette()] {
        commands.set_theme(palette::theme(library.palette()));
    }
}

#[system(order = Animation)]
fn scene_animation(
    state: model!(model::DisplayState),
    delta: res!(crate::ecs::DeltaTimeMs),
    theme: res!(crate::ui::Theme),
) {
    if !state.launcher().active() {
        state.animate(delta.0, &theme);
    }
}

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_least(360, 480);
