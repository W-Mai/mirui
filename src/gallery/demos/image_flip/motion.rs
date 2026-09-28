use crate::ecs::DeltaTimeMs;
use crate::prelude::*;
use crate::types::Transform3D;
use crate::ui::widgets::WidgetTransform3D;

pub struct Spinner {
    pub angle: Fixed,
    pub speed_deg_per_second: Fixed,
    pub bounce_phase: Fixed,
}

//~focus-start
#[mirui_macros::system(order = ANIMATION)]
pub fn spin_system(world: &mut World) {
    let dt = world
        .resource::<DeltaTimeMs>()
        .map_or(16, |delta| delta.0)
        .min(50);
    world.for_each_stable::<Spinner>(|world, e| {
        let (angle, bounce) = if let Some(s) = world.get_mut::<Spinner>(e) {
            let step = s.speed_deg_per_second * Fixed::from_ratio(i32::from(dt), 1_000);
            s.angle += step;
            if s.angle >= Fixed::from_int(360) {
                s.angle -= Fixed::from_int(360);
            }
            s.bounce_phase += step;
            if s.bounce_phase >= Fixed::from_int(360) {
                s.bounce_phase -= Fixed::from_int(360);
            }
            (s.angle, s.bounce_phase)
        } else {
            return;
        };

        let t_num = bounce.to_int() % 180;
        let t = Fixed::from_int(t_num) / Fixed::from_int(180);
        let two_t_minus_1 = t * Fixed::from_int(2) - Fixed::ONE;
        let h = Fixed::ONE - two_t_minus_1 * two_t_minus_1;

        let bounce_y = Fixed::ZERO - h * Fixed::from_int(100);
        let squash = Fixed::ONE - (Fixed::ONE - h) / Fixed::from_int(4);
        let stretch = Fixed::ONE + h / Fixed::from_int(8);

        let rot = Transform3D::rotate_y_perspective(angle, Fixed::from_int(400));
        let scale = Transform3D::scale(squash, stretch);
        let translate = Transform3D::translate(Fixed::ZERO, bounce_y);
        world.insert(
            e,
            WidgetTransform3D(translate.compose(&rot).compose(&scale)),
        );
        world.invalidate(e);
    });
}
//~focus-end
