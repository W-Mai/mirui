use super::state::Page;
use crate::ecs::DeltaTimeMs;
use crate::prelude::*;
use crate::types::Transform3D;
use crate::ui::widgets::WidgetTransform3D;

#[mirui_macros::system(order = ANIMATION)]
pub fn flip_system(world: &mut World) {
    let dt = world
        .resource::<DeltaTimeMs>()
        .map_or(16, |delta| delta.0)
        .min(50);
    world.for_each_stable::<Page>(|world, e| {
        let angle = if let Some(p) = world.get_mut::<Page>(e) {
            p.angle_deg += p.speed_deg_per_second * Fixed::from_ratio(i32::from(dt), 1_000);
            let limit = Fixed::from_int(120);
            if p.angle_deg > limit {
                p.angle_deg = limit * 2 - p.angle_deg;
                p.speed_deg_per_second = -p.speed_deg_per_second;
            } else if p.angle_deg < Fixed::ZERO {
                p.angle_deg = -p.angle_deg;
                p.speed_deg_per_second = -p.speed_deg_per_second;
            }
            p.angle_deg
        } else {
            return;
        };
        world.insert(
            e,
            WidgetTransform3D(Transform3D::rotate_y_perspective(
                Fixed::ZERO - angle,
                Fixed::from_int(500),
            )),
        );
        world.invalidate(e);
    });
}
