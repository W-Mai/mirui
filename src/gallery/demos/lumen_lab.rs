extern crate alloc;

mod board;
#[cfg(feature = "persistence")]
mod persistence;
mod runtime;
mod shell;
mod stars;
mod style;

#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::gallery::play::font::PlayFontPlugin;
use crate::gallery::play::lumen::LumenModel;
use crate::prelude::*;

use self::board::board_render;
use self::runtime::{keyboard, lumen_tick_system};
use self::shell::build_widgets;
use self::stars::stars_render;

pub const VIEWPORT: (u16, u16) = (480, 320);

pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    #[cfg(feature = "std")]
    app.add_plugin(StdInstantClockPlugin);
    app.add_plugin(PlayFontPlugin);
    #[cfg(feature = "persistence")]
    persistence::install_persistence(app);
    app.with_widget(board_render::view())
        .with_widget(stars_render::view());
    let model = app.add_model(LumenModel::new()).finish();
    app.on_event(Key::EVENT, keyboard::callback(model.clone()));
    app.add_system(lumen_tick_system::system(model.clone()));
    app.compose(parent, build_widgets(model));
}

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::fixed(480, 320);

#[cfg(test)]
mod tests;
