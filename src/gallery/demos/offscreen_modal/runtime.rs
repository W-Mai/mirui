#[cfg(feature = "std")]
use super::animation::{fps_readout_system, modal_slide_system, mode_toggle_system};
#[cfg(feature = "std")]
use super::composition::build_widgets;
#[cfg(feature = "std")]
use super::state::ModeToggle;
#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
#[cfg(feature = "std")]
use crate::ecs::Entity;
#[cfg(feature = "std")]
use crate::prelude::*;

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    // Modal buffer at RGBA8888 = 200×280×4 = 224 KB; the 256 KiB pool
    // fits one buffer with eviction headroom.
    app.with_offscreen_pool_budget(256 * 1024);
    app.world.insert_resource(ModeToggle {
        last_flip_ns: 0,
        elapsed_ns: 0,
        offscreen: false,
    });
    app.add_system(modal_slide_system::system());
    app.add_system(mode_toggle_system::system());
    app.add_system(fps_readout_system::system());
    app.add_plugin(StdInstantClockPlugin);
    app.compose(parent, build_widgets);
}
