#[cfg(feature = "std")]
use super::render::life_view;
use super::state::{LifeBoard, MAX_GRID_EDGE};
use crate::prelude::*;

const PX_PER_CELL: i32 = 1;
const MIN_GRID_EDGE: i32 = 48;

#[mirui_macros::system]
pub fn life_step_system(world: &mut World) {
    world.for_each_stable::<LifeBoard>(|world, e| {
        // re-grid to the laid-out size: cell count tracks the canvas, no stretch
        if let Some(rect) = world.get::<crate::ui::ComputedRect>(e).map(|c| c.0) {
            let (cols, rows) = dims_from_px(rect.w.to_int(), rect.h.to_int());
            if let Some(b) = world.get_mut::<LifeBoard>(e) {
                b.resize(cols, rows);
            }
        }
        if let Some(b) = world.get_mut::<LifeBoard>(e) {
            b.advance();
        }
        world.invalidate(e);
    });
}

pub(in crate::gallery::demos) fn dims_from_px(w: i32, h: i32) -> (i32, i32) {
    let longest = w.max(h).max(1);
    let scale = PX_PER_CELL.max((longest + MAX_GRID_EDGE - 1) / MAX_GRID_EDGE);
    let cols = (w / scale).clamp(MIN_GRID_EDGE, MAX_GRID_EDGE);
    let rows = (h / scale).clamp(MIN_GRID_EDGE, MAX_GRID_EDGE);
    (cols, rows)
}

mirui_macros::timer!(LifeTick, every: 90, |world, _entity| {
    life_step_system(world);
});

#[cfg(feature = "std")]
pub(in crate::gallery::demos) fn install_runtime<B, F>(app: &mut App<B, F>)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::app::plugins::StdInstantClockPlugin;
    use crate::prelude::plugin::FpsSummaryPlugin;
    app.with_widget(life_view())
        .add_plugin(StdInstantClockPlugin)
        .add_plugin(FpsSummaryPlugin::default());
    LifeTick::install(&mut app.world);
}
