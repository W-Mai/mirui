use super::render::life_render;
use super::state::{LifeBoard, MAX_GRID_EDGE};
use crate::prelude::*;

const PX_PER_CELL: i32 = 1;
const MIN_GRID_EDGE: i32 = 48;

#[crate::component]
pub(super) struct LifeStepDue;

#[crate::system]
pub fn life_step_system(
    ticks: query!(Entity; with LifeStepDue;),
    boards: query!(mut LifeBoard, optional crate::ui::ComputedRect),
    commands: commands!(),
) {
    let mut due = false;
    for timer in ticks {
        commands.remove::<LifeStepDue>(timer);
        due = true;
    }
    if !due {
        return;
    }

    for (board, computed) in boards {
        // Re-grid to the laid-out size: cell count tracks the canvas, no stretch.
        if let Some(rect) = computed.map(|computed| computed.0) {
            let (cols, rows) = dims_from_px(rect.w.to_int(), rect.h.to_int());
            board.resize(cols, rows);
        }
        board.advance();
    }
}

pub(in crate::gallery::demos) fn dims_from_px(w: i32, h: i32) -> (i32, i32) {
    let longest = w.max(h).max(1);
    let scale = PX_PER_CELL.max((longest + MAX_GRID_EDGE - 1) / MAX_GRID_EDGE);
    let cols = (w / scale).clamp(MIN_GRID_EDGE, MAX_GRID_EDGE);
    let rows = (h / scale).clamp(MIN_GRID_EDGE, MAX_GRID_EDGE);
    (cols, rows)
}

#[mirui_macros::timer(every = 90)]
fn life_tick(timer: entity!(), commands: commands!()) {
    commands.insert(timer, LifeStepDue);
}

pub(in crate::gallery::demos) fn install_runtime<B, F>(app: &mut App<B, F>)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.with_widget(life_render::view());
    #[cfg(feature = "std")]
    {
        use crate::app::plugins::StdInstantClockPlugin;
        use crate::prelude::plugin::FpsSummaryPlugin;
        app.add_plugin(StdInstantClockPlugin)
            .add_plugin(FpsSummaryPlugin::default());
    }
    app.add_system(life_step_system::system());
    app.add_timer(life_tick::timer());
}
