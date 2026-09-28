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

use self::board::board_view;
use self::runtime::{LumenNodes, lumen_tick_system};
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
    app.with_widget(board_view());
    app.world.insert_resource(LumenModel::new());
    app.add_system(lumen_tick_system::system());
    app.compose(parent, build_widgets);
    let find = |id| {
        app.world
            .find_by_id(id)
            .unwrap_or_else(|| panic!("missing {id}"))
    };
    app.world.insert_resource(LumenNodes {
        board: find("lumen_board"),
        level_name: find("lumen_level_name"),
        level_subtitle: find("lumen_level_subtitle"),
        puzzle: find("lumen_puzzle"),
        status: find("lumen_status"),
        status_subtitle: find("lumen_status_subtitle"),
        moves: find("lumen_moves"),
        completed: find("lumen_completed"),
        hint_note: find("lumen_hint_note"),
        undo: find("lumen_undo"),
        scan: find("lumen_scan"),
        next: find("lumen_next"),
        modal: find("lumen_levels_modal"),
        mirror_labels: [
            find("lumen_mirror_1"),
            find("lumen_mirror_2"),
            find("lumen_mirror_3"),
            find("lumen_mirror_4"),
            find("lumen_mirror_5"),
            find("lumen_mirror_6"),
            find("lumen_mirror_7"),
        ],
        level_checks: [
            find("lumen_level_check_1"),
            find("lumen_level_check_2"),
            find("lumen_level_check_3"),
            find("lumen_level_check_4"),
            find("lumen_level_check_5"),
        ],
    });
    LumenNodes::sync(&mut app.world);
}

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::fixed(480, 320);

#[cfg(test)]
mod tests;
