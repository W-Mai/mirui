use super::state::{BAR_H, BAR_MARGIN, BAR_W, BarArena, BarState, START_Y};
use crate::prelude::*;
use crate::ui;

//~focus-start
#[mirui_macros::system(order = ANIMATION)]
pub fn bar_move_system(world: &mut World) {
    let delta_ms = world
        .resource::<crate::ecs::DeltaTimeMs>()
        .expect("Subpixel animation requires DeltaTimeMs")
        .0;
    let Some(arena) = world.resource::<BarArena>().copied() else {
        return;
    };
    let Some(bounds) = world.get::<ui::ComputedRect>(arena.0).map(|rect| rect.0) else {
        return;
    };
    let elapsed = Fixed::from_ratio(delta_ms as i32, 1_000);
    let bound_w = bounds.w.to_int();
    let bound_h = bounds.h.to_int();
    world.for_each_stable::<BarState>(|world, e| {
        let (new_x, new_y, changed) = {
            let Some(bar) = world.get_mut::<BarState>(e) else {
                return;
            };
            if bar.right_anchored {
                bar.x = Fixed::from_int((bound_w - BAR_W - BAR_MARGIN).max(BAR_MARGIN));
            }
            let old_display = if bar.snap { bar.y.floor() } else { bar.y };
            bar.y += bar.speed_per_second * elapsed;
            if bar.y > Fixed::from_int((bound_h - BAR_H - BAR_MARGIN).max(START_Y)) {
                bar.y = Fixed::from_int(START_Y);
            }
            let new_display = if bar.snap { bar.y.floor() } else { bar.y };
            (
                bar.x,
                new_display,
                new_display != old_display || bar.right_anchored,
            )
        };
        if changed {
            ui::set_position(world, e, new_x, new_y);
        }
    });
}
//~focus-end
