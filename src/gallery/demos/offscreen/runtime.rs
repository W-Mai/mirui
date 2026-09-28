use super::composition::build_widgets;
use super::state::ModeToggle;
use super::systems::{force_dirty_system, fps_readout_system, mode_toggle_system};
use crate::app::plugins::StdInstantClockPlugin;
use crate::ecs::Entity;
use crate::prelude::*;

pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.with_offscreen_pool_budget(512 * 1024);
    app.world.insert_resource(ModeToggle {
        last_flip_ns: 0,
        elapsed_ns: 0,
        offscreen: false,
    });
    app.add_system(mode_toggle_system::system());
    app.add_system(force_dirty_system::system());
    app.add_system(fps_readout_system::system());
    app.add_plugin(StdInstantClockPlugin);
    app.compose(parent, build_widgets);
}
