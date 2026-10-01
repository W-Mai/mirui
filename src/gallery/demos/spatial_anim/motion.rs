use crate::anim::Spring;
use crate::ecs::DeltaTimeMs;
use crate::prelude::{Dimension, Fixed};
use crate::ui::Style;

#[mirui_macros::animate(component = AnimateTweenY)]
pub fn apply_tween_y(value: motion!(), style: component!(mut Style)) {
    style.layout.left = Dimension::Px(Fixed::from_int(48));
    style.layout.top = Dimension::Px(value);
}

#[crate::component]
pub struct SpringBall {
    pub spring: Spring,
    pub x: Fixed,
}

//~focus-start
#[mirui_macros::system(order = Animation)]
pub fn spring_system(delta: res!(DeltaTimeMs), balls: query!(mut SpringBall, mut Style)) {
    for (ball, style) in balls {
        ball.spring.tick(delta.0);
        let pos = ball.spring.value();
        let settled = ball.spring.is_settled();
        let target = ball.spring.target;
        style.layout.left = Dimension::Px(ball.x);
        style.layout.top = Dimension::Px(pos);
        if settled {
            let new_target = if target.to_int() > 150 {
                Fixed::from_int(48)
            } else {
                Fixed::from_int(220)
            };
            ball.spring.retarget(new_target, None);
        }
    }
}
//~focus-end
