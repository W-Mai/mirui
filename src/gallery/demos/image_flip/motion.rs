use crate::ecs::DeltaTimeMs;
use crate::prelude::*;
use crate::types::Transform3D;
use crate::ui::dirty::Dirty;
use crate::ui::widgets::WidgetTransform3D;

#[crate::component]
pub struct Spinner {
    pub angle: Fixed,
    pub speed_deg_per_second: Fixed,
    pub bounce_phase: Fixed,
}

//~focus-start
#[mirui_macros::system(order = Animation)]
pub fn spin_system(
    delta: res!(DeltaTimeMs),
    spinners: query!(Entity, mut Spinner),
    commands: commands!(),
) {
    let dt = delta.0.min(50);
    for (entity, spinner) in spinners {
        let step = spinner.speed_deg_per_second * Fixed::from_ratio(i32::from(dt), 1_000);
        spinner.angle += step;
        if spinner.angle >= Fixed::from_int(360) {
            spinner.angle -= Fixed::from_int(360);
        }
        spinner.bounce_phase += step;
        if spinner.bounce_phase >= Fixed::from_int(360) {
            spinner.bounce_phase -= Fixed::from_int(360);
        }

        let t_num = spinner.bounce_phase.to_int() % 180;
        let t = Fixed::from_int(t_num) / Fixed::from_int(180);
        let two_t_minus_1 = t * Fixed::from_int(2) - Fixed::ONE;
        let h = Fixed::ONE - two_t_minus_1 * two_t_minus_1;

        let bounce_y = Fixed::ZERO - h * Fixed::from_int(100);
        let squash = Fixed::ONE - (Fixed::ONE - h) / Fixed::from_int(4);
        let stretch = Fixed::ONE + h / Fixed::from_int(8);

        let rot = Transform3D::rotate_y_perspective(spinner.angle, Fixed::from_int(400));
        let scale = Transform3D::scale(squash, stretch);
        let translate = Transform3D::translate(Fixed::ZERO, bounce_y);
        commands.insert(
            entity,
            WidgetTransform3D(translate.compose(&rot).compose(&scale)),
        );
        commands.insert(entity, Dirty);
    }
}
//~focus-end
