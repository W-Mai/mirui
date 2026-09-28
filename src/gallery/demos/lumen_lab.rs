extern crate alloc;

mod board;
mod runtime;
mod shell;
mod style;

#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::gallery::play::font::register_play_font;
use crate::gallery::play::lumen::LumenModel;
use crate::prelude::*;

use self::board::board_render;
use self::runtime::lumen_tick_system;
use self::shell::build_widgets;

pub const VIEWPORT: (u16, u16) = (480, 320);

pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    #[cfg(feature = "std")]
    app.add_plugin(StdInstantClockPlugin);
    register_play_font(&mut app.world);
    app.with_widget(board_render::view());
    let model = app.add_model(LumenModel::new());
    app.add_system(lumen_tick_system::system(model.clone()));
    app.compose(parent, |cx| build_widgets(cx, model));
}

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::fixed(480, 320);

#[cfg(test)]
mod tests;
