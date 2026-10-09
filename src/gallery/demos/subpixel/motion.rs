use super::state::{BAR_H, BAR_MARGIN, BAR_W, BarState, START_Y};
use crate::prelude::*;
use crate::ui::property::prop;
use crate::ui::{ComputedRect, Parent};

//~focus-start
#[mirui_macros::system(order = Animation)]
pub fn bar_move_system(
    delta: res!(crate::ecs::DeltaTimeMs),
    rects: query!(ComputedRect),
    bars: query!(Entity, mut BarState, Parent),
    commands: commands!(),
) {
    let elapsed = Fixed::from_ratio(delta.0 as i32, 1_000);
    for (entity, bar, parent) in bars {
        let Some(bounds) = rects.get(parent.0) else {
            continue;
        };
        let bound_w = bounds.0.w.to_int();
        let bound_h = bounds.0.h.to_int();
        if bar.right_anchored {
            bar.x = Fixed::from_int((bound_w - BAR_W - BAR_MARGIN).max(BAR_MARGIN));
        }
        let old_display = if bar.snap { bar.y.floor() } else { bar.y };
        bar.y += bar.speed_per_second * elapsed;
        if bar.y > Fixed::from_int((bound_h - BAR_H - BAR_MARGIN).max(START_Y)) {
            bar.y = Fixed::from_int(START_Y);
        }
        let new_display = if bar.snap { bar.y.floor() } else { bar.y };
        let changed = new_display != old_display || bar.right_anchored;
        if changed {
            commands.set::<prop::Left>(entity, Dimension::Px(bar.x));
            commands.set::<prop::Top>(entity, Dimension::Px(new_display));
        }
    }
}
//~focus-end
