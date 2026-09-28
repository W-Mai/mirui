use super::composition::build_widgets;
use super::motion::bar_move_system;
use super::state::BarArena;
use crate::prelude::*;

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.add_system(bar_move_system::system());
    app.compose(parent, build_widgets);
    let arena = app
        .world
        .find_by_id("subpixel_arena")
        .expect("subpixel arena");
    app.world.insert_resource(BarArena(arena));
}
