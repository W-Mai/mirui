use super::state::Page;
use crate::ecs::DeltaTimeMs;
use crate::prelude::*;
use crate::types::Transform3D;
use crate::ui::dirty::Dirty;
use crate::ui::widgets::WidgetTransform3D;

#[mirui_macros::system(order = Animation)]
pub fn flip_system(
    delta: res!(DeltaTimeMs),
    pages: query!(Entity, mut Page),
    commands: commands!(),
) {
    let dt = delta.0.min(50);
    for (entity, page) in pages {
        page.angle_deg += page.speed_deg_per_second * Fixed::from_ratio(i32::from(dt), 1_000);
        let limit = Fixed::from_int(120);
        if page.angle_deg > limit {
            page.angle_deg = limit * 2 - page.angle_deg;
            page.speed_deg_per_second = -page.speed_deg_per_second;
        } else if page.angle_deg < Fixed::ZERO {
            page.angle_deg = -page.angle_deg;
            page.speed_deg_per_second = -page.speed_deg_per_second;
        }
        commands.insert(
            entity,
            WidgetTransform3D(Transform3D::rotate_y_perspective(
                Fixed::ZERO - page.angle_deg,
                Fixed::from_int(500),
            )),
        );
        commands.insert(entity, Dirty);
    }
}
